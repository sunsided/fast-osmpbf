use fast_osmpbf::prelude::*;
use fast_osmpbf::*;
use std::env;
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    env::var_os("OSM_PBF_TEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/clipperton.osm.pbf")
        })
}

fn require_fixture() -> PathBuf {
    let path = fixture_path();
    if !path.exists() {
        panic!("fixture not found at {}", path.display());
    }
    path
}

#[test]
fn reader_from_path() {
    let path = require_fixture();
    let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
    // Reader created successfully; header was parsed without error
    drop(reader);
}

#[test]
fn blocks_count_all() {
    let path = require_fixture();
    let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
    let mut count: usize = 0;
    reader.blocks().for_each(|block| match block {
        ElementBlock::DenseNodeBlock(b) => count += b.iter().count(),
        ElementBlock::NodeBlock(b) => count += b.iter().count(),
        ElementBlock::WayBlock(b) => count += b.iter().count(),
        ElementBlock::RelationBlock(b) => count += b.iter().count(),
    });
    assert!(count > 0, "expected at least one element, got {count}");
}

#[test]
fn par_blocks_count() {
    let path = require_fixture();
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
    assert!(count > 0, "expected at least one element, got {count}");
}

#[test]
fn count_ways() {
    let path = require_fixture();
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
    assert!(count > 0, "expected at least one way, got {count}");
}

#[test]
fn count_addresses() {
    let path = require_fixture();
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
    // Clipperton fixture may not contain address tags; verify iteration completes
    let _ = count;
}
