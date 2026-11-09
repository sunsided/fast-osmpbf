# fast-osmpbf

A high-performance Rust library for reading OpenStreetMap files ([\*.osm.pbf](https://wiki.openstreetmap.org/wiki/PBF_Format)).
This library is focused on performance and provides pretty high-level data. if you need some lower level metadata,
you might want to check out other libraries such as [osmpbf](https://crates.io/crates/osmpbf).

[![Crates.io](https://img.shields.io/crates/v/osmpbf.svg)](https://crates.io/crates/fast-osmpbf)
[![Documentation](https://docs.rs/fast-osmpbf/badge.svg)](https://docs.rs/fast-osmpbf)

## Benchmarks

- CPU: Intel(R) Core(TM) i5-9600K CPU @ 3.70GHz
- Memory: 16GB
- OS: Linux (Ubuntu)
- Dataset: `germany-latest.osm.pbf` (~4.6GB)

```text
Hyperfine was used to benchmark with 10 runs each.
What you see here is the mean time of these 10 runs.
All problems were run using parallelization if the library offered a way to do it.
```

| Problem         | fast-osmpbf | osmpbf  | osmpbfreader |
| --------------- | ----------- | ------- | ------------ |
| Count ways      | 13.51 s     | 19.06 s | 39.58 s      |
| Count addresses | 15.33 s     | 21.70 s | 45.45 s      |

## Examples

1. Count ways.
   We can (but dont have to) apply an element filter beforehand. Saves a little bit of computation.

```rust,ignore
use fast_osmpbf::*;

fn main() {
    let arg = std::env::args_os()
        .nth(1)
        .expect("need a *.osm.pbf file as argument");
    let path = std::path::Path::new(&arg);
    let reader = OsmReader::from_path(path).expect("Invalid file path");

    // apply filter
    reader.apply_element_filter(ElementFilter { nodes: false, relations: false, ways: true });

    // iterate using .blocks() (Parallelization happens, but only for one decoding step)
    let mut way_counter = 0;
    reader.blocks().for_each(|block| match block {
        ElementBlock::WayBlock(block) => {
            for _way in block.iter() {
                way_counter += 1;
            }
        }
        _ => (),
    });
    println!("Ways: {:?}", way_counter);
}
```

2. Count elements that have full addresses.
   We apply a filter on tags beforehand. This filter does not only filtering on tags, it actually speeds up computing by using a cache.
   If you iterate over tags and you know you only need certain tags, apply the filter beforehand.

```rust,ignore
use fast_osmpbf::*;
use fast_osmpbf::prelude::*;

pub fn main() {
    let arg = std::env::args_os()
        .nth(1)
        .expect("need a *.osm.pbf file as argument");
    let path = std::path::Path::new(&arg);
    let reader = OsmReader::from_path(path).expect("Invalid file path");

    // apply filter
    reader
        .apply_tag_filter(&[
            "addr:city",
            "addr:postcode",
            "addr:street",
            "addr:housenumber",
        ])
        .expect("Invalid filter applied");

    // iterate using .par_blocks() (Parallelization happens on 2 decoding steps)
    let address_counter: usize = reader
        .par_blocks()
        .map(|block| match block {
            ElementBlock::DenseNodeBlock(block) => block
                .iter()
                .filter(|node| node.tags().has_all_filter_keys())
                .count(),
            ElementBlock::NodeBlock(block) => block
                .iter()
                .filter(|node| node.tags().has_all_filter_keys())
                .count(),
            ElementBlock::WayBlock(block) => block
                .iter()
                .filter(|way| way.tags().has_all_filter_keys())
                .count(),
            ElementBlock::RelationBlock(block) => block
                .iter()
                .filter(|rel| rel.tags().has_all_filter_keys())
                .count(),
        })
        .sum();
    println!("Addresses: {:?}", address_counter);
}
```

## Benchmarks & Profiling

This crate includes a Criterion benchmark suite and convenient flamegraph
profiling aliases.

### Running benchmarks

```bash
# Run the full benchmark suite
cargo bench-parse

# Use a larger fixture
OSM_PBF_BENCH=/path/to/larger.osm.pbf cargo bench-parse
```

Benchmarks resolve the input file relative to `CARGO_MANIFEST_DIR` or the
`OSM_PBF_BENCH` environment variable. If the fixture is missing, benches print
a warning and skip rather than panicking.

Benchmarks and integration tests also read `OSM_PBF_BENCH` and `OSM_PBF_TEST`
from a git-ignored `.env` file. Variables already set in the environment take
precedence.

### Flamegraph profiling

Requires [`cargo-flamegraph`](https://github.com/flamegraph-rs/flamegraph)
(`cargo install flamegraph`) and `perf` (Linux) or `dtrace` (macOS).

```bash
# Profile the count_ways example
cargo flame-ways

# Profile the count_addresses example
cargo flame-addresses

# Profile the benchmark suite
cargo flame-bench
```

Each command produces `flamegraph.svg` in the current directory.

## Features

This crate supports the following optional Cargo features:

| Feature | Description |
| ------- | ----------- |
| `mmap` | Enable memory-mapped file I/O via `from_path_mmap` for potentially faster reads on large files. |
| `io-hints` | Enable platform-specific I/O hints (e.g. `fadvise` on Linux, `FILE_FLAG_SEQUENTIAL_SCAN` on Windows) to optimize sequential file reads. |

### Using the `mmap` feature

When the `mmap` feature is enabled, you can use `OsmReader::from_path_mmap` to memory-map the PBF file instead of using buffered I/O:

```toml
[dependencies]
fast-osmpbf = { version = "0.2", features = ["mmap"] }
```

```rust,ignore
use fast_osmpbf::*;

fn main() {
    let arg = std::env::args_os()
        .nth(1)
        .expect("need a *.osm.pbf file as argument");
    let path = std::path::Path::new(&arg);
    let reader = OsmReader::from_path_mmap(path).expect("Invalid file path");

    reader.blocks().for_each(|block| {
        // process blocks...
    });
}
```

### Using the `io-hints` feature

When the `io-hints` feature is enabled, both `from_path` and `from_path_mmap` automatically apply OS-level sequential read hints. These hints improve throughput on large files by telling the kernel to prefetch pages and avoid caching read-once data:

```toml
[dependencies]
fast-osmpbf = { version = "0.2", features = ["io-hints"] }
```

```rust,ignore
use fast_osmpbf::*;

fn main() {
    let arg = std::env::args_os()
        .nth(1)
        .expect("need a *.osm.pbf file as argument");
    let path = std::path::Path::new(&arg);

    // Buffered I/O with sequential read hints
    let reader = OsmReader::from_path(path).expect("Invalid file path");

    // Or combine with mmap for maximum throughput
    // let reader = OsmReader::from_path_mmap(path).expect("Invalid file path");

    reader.blocks().for_each(|block| {
        // process blocks...
    });
}
```

For best results, combine both features:

```toml
[dependencies]
fast-osmpbf = { version = "0.2", features = ["mmap", "io-hints"] }
```

## License

This project is licensed under

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  http://opensource.org/licenses/MIT)
