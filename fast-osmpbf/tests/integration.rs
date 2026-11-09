use fast_osmpbf::prelude::*;
use fast_osmpbf::*;
use std::path::PathBuf;
use std::sync::Mutex;

const FIXTURE_TOTAL: usize = 3265;
const FIXTURE_DENSE_NODES: usize = 3097;
const FIXTURE_WAYS: usize = 151;
const FIXTURE_RELATIONS: usize = 17;

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn fixture_path() -> PathBuf {
    dotenvy::var("OSM_PBF_TEST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
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
    let _guard = TEST_LOCK.lock().unwrap();
    let path = require_fixture();
    let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
    drop(reader);
}

#[test]
fn blocks_count_all() {
    let _guard = TEST_LOCK.lock().unwrap();
    let path = require_fixture();
    let reader = OsmReader::from_path(&path).expect("Failed to open fixture");
    let mut count: usize = 0;
    let mut dense_nodes = 0;
    let mut ways = 0;
    let mut relations = 0;
    reader.blocks().for_each(|block| match block {
        ElementBlock::DenseNodeBlock(b) => {
            let c = b.iter().count();
            dense_nodes += c;
            count += c;
        }
        ElementBlock::NodeBlock(b) => {
            let c = b.iter().count();
            count += c;
        }
        ElementBlock::WayBlock(b) => {
            let c = b.iter().count();
            ways += c;
            count += c;
        }
        ElementBlock::RelationBlock(b) => {
            let c = b.iter().count();
            relations += c;
            count += c;
        }
    });
    assert_eq!(count, FIXTURE_TOTAL, "total element count mismatch");
    assert_eq!(
        dense_nodes, FIXTURE_DENSE_NODES,
        "dense node count mismatch"
    );
    assert_eq!(ways, FIXTURE_WAYS, "way count mismatch");
    assert_eq!(relations, FIXTURE_RELATIONS, "relation count mismatch");
}

#[test]
fn par_blocks_count() {
    let _guard = TEST_LOCK.lock().unwrap();
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
    assert_eq!(
        count, FIXTURE_TOTAL,
        "total element count mismatch (par_blocks)"
    );
}

#[test]
fn count_ways() {
    let _guard = TEST_LOCK.lock().unwrap();
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
    // Reset global filter so subsequent tests aren't affected
    OsmReader::from_path(&path)
        .expect("Failed to open fixture")
        .apply_element_filter(ElementFilter {
            nodes: true,
            ways: true,
            relations: true,
        });
    assert_eq!(
        count, FIXTURE_WAYS,
        "expected exactly {FIXTURE_WAYS} ways, got {count}"
    );
}

#[test]
fn count_addresses() {
    let _guard = TEST_LOCK.lock().unwrap();
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
    let _ = count;
}

#[cfg(feature = "mmap")]
#[test]
fn blocks_count_all_mmap() {
    let _guard = TEST_LOCK.lock().unwrap();
    let path = require_fixture();
    let reader = OsmReader::from_path_mmap(&path).expect("Failed to open fixture");
    let mut count: usize = 0;
    let mut dense_nodes = 0;
    let mut ways = 0;
    let mut relations = 0;
    reader.blocks().for_each(|block| match block {
        ElementBlock::DenseNodeBlock(b) => {
            let c = b.iter().count();
            dense_nodes += c;
            count += c;
        }
        ElementBlock::NodeBlock(b) => {
            let c = b.iter().count();
            count += c;
        }
        ElementBlock::WayBlock(b) => {
            let c = b.iter().count();
            ways += c;
            count += c;
        }
        ElementBlock::RelationBlock(b) => {
            let c = b.iter().count();
            relations += c;
            count += c;
        }
    });
    assert_eq!(count, FIXTURE_TOTAL, "total element count mismatch (mmap)");
    assert_eq!(
        dense_nodes, FIXTURE_DENSE_NODES,
        "dense node count mismatch (mmap)"
    );
    assert_eq!(ways, FIXTURE_WAYS, "way count mismatch (mmap)");
    assert_eq!(
        relations, FIXTURE_RELATIONS,
        "relation count mismatch (mmap)"
    );
}

#[cfg(feature = "mmap")]
#[test]
fn par_blocks_count_mmap() {
    let _guard = TEST_LOCK.lock().unwrap();
    let path = require_fixture();
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
    assert_eq!(
        count, FIXTURE_TOTAL,
        "total element count mismatch (par_blocks mmap)"
    );
}

#[cfg(feature = "mmap")]
#[test]
fn count_ways_mmap() {
    let _guard = TEST_LOCK.lock().unwrap();
    let path = require_fixture();
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
    OsmReader::from_path_mmap(&path)
        .expect("Failed to open fixture")
        .apply_element_filter(ElementFilter {
            nodes: true,
            ways: true,
            relations: true,
        });
    assert_eq!(
        count, FIXTURE_WAYS,
        "expected exactly {FIXTURE_WAYS} ways (mmap), got {count}"
    );
}
