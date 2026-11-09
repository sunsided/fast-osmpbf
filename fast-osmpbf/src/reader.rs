use quick_protobuf::{BytesReader, MessageRead};
use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::{
    BlobHeader, ELEMENT_FILTER, ElementBlock, ElementBlockIter, ElementFilter, TAG_KEYS_FILTER,
    parser::OsmParser,
};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
    sync::Arc,
};

#[cfg(feature = "mmap")]
use std::io::ErrorKind;

#[cfg(feature = "io-hints")]
use crate::io_hints::open_sequential_read_only;

#[cfg(not(feature = "io-hints"))]
use std::fs::OpenOptions;

#[cfg(not(feature = "io-hints"))]
#[inline]
fn open_file<P: AsRef<Path>>(path: P) -> std::io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

#[cfg(feature = "io-hints")]
#[inline]
fn open_file<P: AsRef<Path>>(path: P) -> std::io::Result<File> {
    open_sequential_read_only(path)
}

const BUF_SIZE: usize = 4 * 1024 * 1024; // 4MB
const MAX_HEADER_SIZE: usize = 64 * 1024; // 64KB
const MAX_Q_ELEMENTS: usize = 1_000;
const MAX_TAGS: usize = 8;

#[derive(Debug)]
pub(crate) enum BlobBytes {
    Owned(Arc<[u8]>),
    #[cfg(feature = "mmap")]
    Mapped {
        owner: Arc<memmap2::Mmap>,
        range: std::ops::Range<usize>,
    },
}

impl BlobBytes {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        match self {
            BlobBytes::Owned(arc) => arc,
            #[cfg(feature = "mmap")]
            BlobBytes::Mapped { owner, range } => &owner[range.clone()],
        }
    }
}

impl std::ops::Deref for BlobBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[derive(Debug)]
enum Source {
    Buffered(BufReader<File>),
    #[cfg(feature = "mmap")]
    Mmap {
        map: Arc<memmap2::Mmap>,
        pos: usize,
    },
}

impl Source {
    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        match self {
            Source::Buffered(reader) => reader.read_exact(buf),
            #[cfg(feature = "mmap")]
            Source::Mmap { map, pos } => {
                let n = buf.len();
                if *pos + n > map.len() {
                    return Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "unexpected EOF",
                    ));
                }
                buf.copy_from_slice(&map[*pos..*pos + n]);
                *pos += n;
                Ok(())
            }
        }
    }

    fn skip(&mut self, n: usize) -> std::io::Result<()> {
        match self {
            Source::Buffered(reader) => reader.seek_relative(n as i64).map(|_| ()),
            #[cfg(feature = "mmap")]
            Source::Mmap { map, pos } => {
                if *pos + n > map.len() {
                    return Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "unexpected EOF",
                    ));
                }
                *pos += n;
                Ok(())
            }
        }
    }

    fn take_blob(&mut self, blob_size: usize) -> std::io::Result<BlobBytes> {
        match self {
            Source::Buffered(reader) => {
                // `Arc::from(Vec)` would still copy, so read directly into the Arc's allocation.
                let mut buf: Arc<[u8]> = std::iter::repeat_n(0u8, blob_size).collect();
                let slice = Arc::get_mut(&mut buf).expect("newly created Arc is unique");
                reader.read_exact(slice)?;
                Ok(BlobBytes::Owned(buf))
            }
            #[cfg(feature = "mmap")]
            Source::Mmap { map, pos } => {
                if *pos + blob_size > map.len() {
                    return Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "unexpected EOF",
                    ));
                }
                let owner = Arc::clone(map);
                let range = *pos..*pos + blob_size;
                *pos += blob_size;
                Ok(BlobBytes::Mapped { owner, range })
            }
        }
    }
}

/// Reader that reads bytes from .osm.pbf file and passes them on to the parser
#[derive(Debug)]
pub struct OsmReader {
    source: Source,
    header: Vec<u8>,
}

impl OsmReader {
    /// Creates a new OsmReader from a file path using buffered I/O
    pub fn from_path<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let path_ref = path.as_ref();
        let path = path.as_ref().to_string_lossy().to_string();
        if !path.ends_with(".osm.pbf") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("File {} is not a .osm.pbf file", path),
            ));
        }
        let file = open_file(path_ref)?;
        let reader = BufReader::with_capacity(BUF_SIZE, file);

        Ok(Self {
            source: Source::Buffered(reader),
            header: Vec::with_capacity(MAX_HEADER_SIZE),
        })
    }

    /// Creates a new OsmReader from a file path using memory-mapped I/O.
    ///
    /// # Safety
    /// Memory-mapping exposes the file contents to the process's address space.
    /// If the underlying file is modified concurrently by another process,
    /// the mmap'd region may reflect those changes without synchronization,
    /// potentially leading to inconsistent reads or undefined behavior.
    ///
    /// To mitigate this, the file is opened read-only with an advisory shared
    /// lock (`LOCK_SH` on Unix, write-sharing denied on Windows), which
    /// prevents other processes that cooperate with advisory locking from
    /// modifying the file while the reader is active.
    ///
    /// # Trade-offs
    /// - **Linux/macOS**: Typically faster for large files; the OS page cache
    ///   handles I/O without extra user-space copies.
    /// - **Windows**: Performance may be worse than buffered I/O for some workloads.
    /// - Requires virtual address space ≥ file size.
    #[cfg(feature = "mmap")]
    pub fn from_path_mmap<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let path_ref = path.as_ref();
        let path = path.as_ref().to_string_lossy().to_string();
        if !path.ends_with(".osm.pbf") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("File {} is not a .osm.pbf file", path),
            ));
        }
        let file = open_file(path_ref)?;
        let map = unsafe { memmap2::Mmap::map(&file)? };

        #[cfg(all(feature = "mmap", feature = "io-hints"))]
        let _ = map.advise(memmap2::Advice::Sequential);

        Ok(Self {
            source: Source::Mmap {
                map: Arc::new(map),
                pos: 0,
            },
            header: Vec::with_capacity(MAX_HEADER_SIZE),
        })
    }

    /// Filters elements (dense_nodes, nodes, ways or relations) depending on the filter provided.
    /// If you only are interested in specific elements, I highly encourage you to use this mechanism
    /// over filtering yourself in the iterator since it not only does the filtering for you,
    /// but actually speeds up computation.
    pub fn apply_element_filter(&self, filter: ElementFilter) {
        *ELEMENT_FILTER.write().unwrap() = Some(filter);
    }

    /// Filters out all tags (key, value) where key is not one of your provided Strings.
    /// If you only are interested in specific tags, I highly encourage you to use this mechanism
    /// over filtering yourself in the iterator since it not only does the filtering for you,
    /// but actually speeds up computation by abusing a caching mechanism.
    /// You can provide between 0 and 8 filter keys.
    pub fn apply_tag_filter(&self, tags: &[&str]) -> Result<(), &'static str> {
        let total_tags = tags.len();
        if total_tags > MAX_TAGS {
            return Err("Not allowed to provide more than 8 tags");
        }

        let mut owned: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        owned.sort_unstable();

        *TAG_KEYS_FILTER.write().unwrap() = Some(owned.into_boxed_slice());
        Ok(())
    }

    /// Creates a parallel iterator that yields [`ElementBlock`]
    pub fn par_blocks(self) -> impl ParallelIterator<Item = ElementBlock> {
        let (blob_tx, blob_rx) = crossbeam_channel::bounded::<BlobBytes>(MAX_Q_ELEMENTS);

        // IO thread: reads raw blobs from file/mmap
        std::thread::spawn(move || {
            let mut reader = self;
            while let Ok(Some(blob)) = reader.next_blob() {
                if blob_tx.send(blob).is_err() {
                    break;
                }
            }
            drop(blob_tx);
        });

        blob_rx.into_iter().par_bridge().flat_map_iter(|blob| {
            OsmParser::deserialize_blob(blob)
                .map(|blocks| blocks.into_iter())
                .unwrap_or_default()
        })
    }

    /// Creates an iterator that yields [`ElementBlock`]
    pub fn blocks(self) -> ElementBlockIter {
        let num_threads = rayon::current_num_threads();
        let (blob_tx, blob_rx) = crossbeam_channel::bounded::<BlobBytes>(num_threads * 4);
        let (element_block_tx, element_block_rx) =
            crossbeam_channel::bounded::<ElementBlock>(MAX_Q_ELEMENTS);

        // IO thread: reads raw blobs from file/mmap
        let io_blob_tx = blob_tx;
        std::thread::spawn(move || {
            let mut reader = self;
            while let Ok(Some(blob)) = reader.next_blob() {
                if io_blob_tx.send(blob).is_err() {
                    break;
                }
            }
            drop(io_blob_tx);
        });

        // Orchestrator thread: parses blobs on the global pool via par_bridge
        std::thread::spawn(move || {
            blob_rx.into_iter().par_bridge().for_each(|blob| {
                if let Ok(element_blocks) = OsmParser::deserialize_blob(blob) {
                    for block in element_blocks {
                        if element_block_tx.send(block).is_err() {
                            return;
                        }
                    }
                }
            });
        });

        ElementBlockIter {
            rx: element_block_rx,
        }
    }

    // Sequential operation - raw blobs have different sizes, need to look at length prefix and blob header first to know exact size
    fn next_blob(&mut self) -> std::io::Result<Option<BlobBytes>> {
        let mut prefix = [0u8; 4];

        // Read length prefix (always 4 bytes)
        if self.source.read_exact(&mut prefix).is_err() {
            return Ok(None); // EOF
        }

        let header_size = u32::from_be_bytes(prefix) as usize;
        if header_size > self.header.capacity() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "BlobHeader size exceeds limit of 64KB. File corrupt?",
            ));
        }

        if self.header.len() < header_size {
            self.header.resize(header_size, 0);
        }
        self.source.read_exact(&mut self.header[..header_size])?;

        // Deserialize blob header to get size of blob
        let mut reader = BytesReader::from_bytes(&self.header[..header_size]);
        let header = BlobHeader::from_reader(&mut reader, &self.header[..header_size])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let blob_size = header.datasize as usize;

        // Skip everything that is not actual relevant data
        if header.type_pb != "OSMData" {
            self.source.skip(blob_size)?;
            return self.next_blob();
        }

        let blob_bytes = self.source.take_blob(blob_size)?;

        Ok(Some(blob_bytes))
    }
}
