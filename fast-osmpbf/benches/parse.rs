use criterion::{Criterion, criterion_group, criterion_main};
use fast_osmpbf::prelude::*;
use fast_osmpbf::*;
use std::path::PathBuf;

fn bench_path() -> PathBuf {
    dotenvy::var("OSM_PBF_BENCH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/clipperton.osm.pbf")
        })
}

fn reader_from_path(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping reader_from_path: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("reader_from_path", |b| {
        b.iter(|| {
            let _reader = OsmReader::from_path(&path).expect("Failed to open fixture");
        });
    });
}

fn blocks_count_all(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping blocks_count_all: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("blocks_count_all", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
            let mut count: usize = 0;
            reader.blocks().for_each(|block| match block {
                ElementBlock::DenseNodeBlock(b) => count += b.iter().count(),
                ElementBlock::NodeBlock(b) => count += b.iter().count(),
                ElementBlock::WayBlock(b) => count += b.iter().count(),
                ElementBlock::RelationBlock(b) => count += b.iter().count(),
            });
            criterion::black_box(count);
        });
    });
}

fn par_blocks_count(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping par_blocks_count: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("par_blocks_count", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
            let count: usize = reader
                .par_blocks()
                .map(|block| match block {
                    ElementBlock::DenseNodeBlock(b) => b.iter().count(),
                    ElementBlock::NodeBlock(b) => b.iter().count(),
                    ElementBlock::WayBlock(b) => b.iter().count(),
                    ElementBlock::RelationBlock(b) => b.iter().count(),
                })
                .sum();
            criterion::black_box(count);
        });
    });
}

fn count_ways(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping count_ways: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("count_ways", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
            reader.apply_element_filter(ElementFilter {
                nodes: false,
                relations: false,
                ways: true,
            });
            let mut count = 0;
            reader.blocks().for_each(|block| match block {
                ElementBlock::WayBlock(block) => {
                    for _way in block.iter() {
                        count += 1;
                    }
                }
                _ => (),
            });
            criterion::black_box(count);
        });
    });
}

fn count_addresses(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping count_addresses: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("count_addresses", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
            reader
                .apply_tag_filter(&[
                    "addr:city",
                    "addr:postcode",
                    "addr:street",
                    "addr:housenumber",
                ])
                .expect("Invalid filter applied");
            let count: usize = reader
                .par_blocks()
                .map(|block| match block {
                    ElementBlock::DenseNodeBlock(b) => {
                        b.iter().filter(|n| n.tags().has_all_filter_keys()).count()
                    }
                    ElementBlock::NodeBlock(b) => {
                        b.iter().filter(|n| n.tags().has_all_filter_keys()).count()
                    }
                    ElementBlock::WayBlock(b) => {
                        b.iter().filter(|w| w.tags().has_all_filter_keys()).count()
                    }
                    ElementBlock::RelationBlock(b) => {
                        b.iter().filter(|r| r.tags().has_all_filter_keys()).count()
                    }
                })
                .sum();
            criterion::black_box(count);
        });
    });
}

#[cfg(feature = "mmap")]
fn blocks_count_all_mmap(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping blocks_count_all_mmap: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("blocks_count_all_mmap", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path_mmap(&path).expect("Failed to open fixture");
            let mut count: usize = 0;
            reader.blocks().for_each(|block| match block {
                ElementBlock::DenseNodeBlock(b) => count += b.iter().count(),
                ElementBlock::NodeBlock(b) => count += b.iter().count(),
                ElementBlock::WayBlock(b) => count += b.iter().count(),
                ElementBlock::RelationBlock(b) => count += b.iter().count(),
            });
            criterion::black_box(count);
        });
    });
}

#[cfg(feature = "mmap")]
fn par_blocks_count_mmap(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping par_blocks_count_mmap: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("par_blocks_count_mmap", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path_mmap(&path).expect("Failed to open fixture");
            let count: usize = reader
                .par_blocks()
                .map(|block| match block {
                    ElementBlock::DenseNodeBlock(b) => b.iter().count(),
                    ElementBlock::NodeBlock(b) => b.iter().count(),
                    ElementBlock::WayBlock(b) => b.iter().count(),
                    ElementBlock::RelationBlock(b) => b.iter().count(),
                })
                .sum();
            criterion::black_box(count);
        });
    });
}

#[cfg(feature = "mmap")]
fn count_ways_mmap(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping count_ways_mmap: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("count_ways_mmap", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path_mmap(&path).expect("Failed to open fixture");
            reader.apply_element_filter(ElementFilter {
                nodes: false,
                relations: false,
                ways: true,
            });
            let mut count = 0;
            reader.blocks().for_each(|block| match block {
                ElementBlock::WayBlock(block) => {
                    for _way in block.iter() {
                        count += 1;
                    }
                }
                _ => (),
            });
            criterion::black_box(count);
        });
    });
}

#[cfg(feature = "mmap")]
fn count_addresses_mmap(c: &mut Criterion) {
    let path = bench_path();
    if !path.exists() {
        eprintln!(
            "Skipping count_addresses_mmap: fixture not found at {}",
            path.display()
        );
        return;
    }
    c.bench_function("count_addresses_mmap", |b| {
        b.iter(|| {
            let reader = OsmReader::from_path_mmap(&path).expect("Failed to open fixture");
            reader
                .apply_tag_filter(&[
                    "addr:city",
                    "addr:postcode",
                    "addr:street",
                    "addr:housenumber",
                ])
                .expect("Invalid filter applied");
            let count: usize = reader
                .par_blocks()
                .map(|block| match block {
                    ElementBlock::DenseNodeBlock(b) => {
                        b.iter().filter(|n| n.tags().has_all_filter_keys()).count()
                    }
                    ElementBlock::NodeBlock(b) => {
                        b.iter().filter(|n| n.tags().has_all_filter_keys()).count()
                    }
                    ElementBlock::WayBlock(b) => {
                        b.iter().filter(|w| w.tags().has_all_filter_keys()).count()
                    }
                    ElementBlock::RelationBlock(b) => {
                        b.iter().filter(|r| r.tags().has_all_filter_keys()).count()
                    }
                })
                .sum();
            criterion::black_box(count);
        });
    });
}

#[cfg(feature = "mmap")]
criterion_group!(
    benches,
    reader_from_path,
    blocks_count_all,
    par_blocks_count,
    count_ways,
    count_addresses,
    blocks_count_all_mmap,
    par_blocks_count_mmap,
    count_ways_mmap,
    count_addresses_mmap,
);
#[cfg(feature = "mmap")]
criterion_main!(benches);

#[cfg(not(feature = "mmap"))]
criterion_group!(
    benches,
    reader_from_path,
    blocks_count_all,
    par_blocks_count,
    count_ways,
    count_addresses,
);
#[cfg(not(feature = "mmap"))]
criterion_main!(benches);
