use crate::{
    Blob, DenseNodeBlock, ElementBlock, NodeBlock, PrimitiveBlock, RelationBlock, WayBlock,
    element::StringTable, get_element_filter, get_tags_filter, reader::BlobBytes,
};
use quick_protobuf::{BytesReader, MessageRead};
use std::io::Read;
use std::sync::Arc;
use std::sync::LazyLock;

static EMPTY_TAG_IDS: LazyLock<Arc<Vec<u32>>> = LazyLock::new(|| Arc::new(Vec::new()));

pub(crate) struct OsmParser;
impl OsmParser {
    pub(crate) fn deserialize_blob(blob_bytes: BlobBytes) -> std::io::Result<Vec<ElementBlock>> {
        let blob_slice = blob_bytes.as_bytes();
        let mut reader = BytesReader::from_bytes(blob_slice);
        let blob = Blob::from_reader(&mut reader, blob_slice)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if let Some(raw) = &blob.raw {
            let data = match &blob_bytes {
                BlobBytes::Owned(arc) => Arc::clone(arc),
                #[cfg(feature = "mmap")]
                BlobBytes::Mapped { owner, range } => Arc::from(&owner[range.clone()]),
            };
            Self::parse_blob(data, raw.as_ref())
        } else if let Some(zlib) = &blob.zlib_data {
            let size = match blob.raw_size {
                Some(raw_size) => raw_size as usize,
                None => blob_slice.len() * 2,
            };
            let mut decompressed = Vec::with_capacity(size);
            let mut decoder = flate2::read::ZlibDecoder::new(&zlib[..]);
            decoder.read_to_end(&mut decompressed)?;
            let data: Arc<[u8]> = Arc::from(decompressed);
            Self::parse_blob(Arc::clone(&data), &data)
        } else if let Some(lzma) = &blob.lzma_data {
            let size = match blob.raw_size {
                Some(raw_size) => raw_size as usize,
                None => blob_slice.len() * 2,
            };
            let mut decompressed = Vec::with_capacity(size);
            let mut decoder = xz2::read::XzDecoder::new(&lzma[..]);
            decoder.read_to_end(&mut decompressed)?;
            let data: Arc<[u8]> = Arc::from(decompressed);
            Self::parse_blob(Arc::clone(&data), &data)
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Empty OSMData blob",
            ));
        }
    }

    fn parse_blob(data: Arc<[u8]>, slice: &[u8]) -> std::io::Result<Vec<ElementBlock>> {
        let mut reader = BytesReader::from_bytes(slice);
        let block = PrimitiveBlock::from_reader(&mut reader, slice)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let base_ptr = data.as_ptr();
        let offsets: Vec<(u32, u32)> = block
            .stringtable
            .s
            .iter()
            .map(|s| {
                let start = unsafe { s.as_ptr().offset_from(base_ptr) } as u32;
                (start, s.len() as u32)
            })
            .collect();

        let stringtable = Arc::new(StringTable {
            data,
            offsets: Arc::from(offsets),
        });

        let tags_filter = get_tags_filter();
        let use_cache = tags_filter.is_some();
        let filter_key_count = tags_filter.as_ref().map_or(0, |f| f.len());
        let cached_tag_ids = match &tags_filter {
            Some(_) => Self::get_tag_ids(&stringtable),
            None => Arc::clone(&EMPTY_TAG_IDS),
        };

        let element_filter = get_element_filter();

        let element_count: usize = block
            .primitivegroup
            .iter()
            .map(|g| {
                g.nodes.len()
                    + g.ways.len()
                    + g.relations.len()
                    + g.dense.as_ref().map_or(0, |d| d.id.len())
            })
            .sum();
        let mut elements: Vec<ElementBlock> = Vec::with_capacity(element_count);

        for group in block.primitivegroup {
            if let Some(dense_nodes) = group.dense
                && element_filter.as_ref().is_none_or(|f| f.nodes)
            {
                let table = Arc::clone(&stringtable);
                elements.push(ElementBlock::DenseNodeBlock(DenseNodeBlock {
                    table,
                    cached_tag_ids: Arc::clone(&cached_tag_ids),
                    use_cache,
                    filter_key_count,
                    granularity: block.granularity,
                    lat_offset: block.lat_offset,
                    lon_offset: block.lon_offset,
                    kv_offsets: Self::compute_offsets(&dense_nodes.keys_vals, dense_nodes.id.len()),
                    nodes: Arc::from(dense_nodes),
                }));
            }
            if !group.nodes.is_empty() && element_filter.as_ref().is_none_or(|f| f.nodes) {
                let table = Arc::clone(&stringtable);
                elements.push(ElementBlock::NodeBlock(NodeBlock {
                    nodes: Arc::from(group.nodes),
                    cached_tag_ids: Arc::clone(&cached_tag_ids),
                    table,
                    use_cache,
                    filter_key_count,
                }));
            }

            if !group.ways.is_empty() && element_filter.as_ref().is_none_or(|f| f.ways) {
                let table = Arc::clone(&stringtable);
                elements.push(ElementBlock::WayBlock(WayBlock {
                    ways: Arc::from(group.ways),
                    cached_tag_ids: Arc::clone(&cached_tag_ids),
                    table,
                    use_cache,
                    filter_key_count,
                }));
            }

            if !group.relations.is_empty() && element_filter.as_ref().is_none_or(|f| f.relations) {
                let table = Arc::clone(&stringtable);
                elements.push(ElementBlock::RelationBlock(RelationBlock {
                    relations: Arc::from(group.relations),
                    cached_tag_ids: Arc::clone(&cached_tag_ids),
                    table,
                    use_cache,
                    filter_key_count,
                }));
            }
        }

        Ok(elements)
    }

    fn get_tag_ids(table: &StringTable) -> Arc<Vec<u32>> {
        let cache = get_tags_filter().unwrap();
        Arc::new(
            table
                .offsets
                .iter()
                .enumerate()
                .filter_map(|(i, &(start, len))| {
                    let key = unsafe {
                        std::str::from_utf8_unchecked(
                            &table.data[start as usize..(start + len) as usize],
                        )
                    };

                    if cache.iter().any(|s| s.as_str() == key) {
                        Some(i as u32)
                    } else {
                        None
                    }
                })
                .collect::<Vec<u32>>(),
        )
    }

    fn compute_offsets(keys_vals: &[i32], node_count: usize) -> Vec<usize> {
        let mut offsets = Vec::with_capacity(node_count + 1);
        offsets.push(0);

        let mut idx = 0;

        for _ in 0..node_count {
            while idx < keys_vals.len() && keys_vals[idx] != 0 {
                idx += 2;
            }

            if idx >= keys_vals.len() {
                offsets.push(idx);
                continue;
            }

            idx += 1;
            offsets.push(idx);
        }

        offsets
    }
}
