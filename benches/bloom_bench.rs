use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode};
use lsm_tree::bloom::{Bloom, RocksDBLocalBloom, SpeedDbDynamicBloom};
use fastbloom::BloomFilter;
use rand::{rngs::StdRng, Rng, SeedableRng};
use xxhash_rust::xxh3::xxh3_128;

fn random_numbers(num: usize, seed: u64) -> Vec<u32> {
    let mut rng = StdRng::seed_from_u64(seed);
    std::iter::repeat_with(|| rng.gen()).take(num).collect()
}

fn bench_bloom_filters(c: &mut Criterion) {
    let sizes = [
        1_000, 2_000, 4_000, 8_000, // Fine-grained small set sizes
        10_000,
        25_000, 50_000, // Medium set sizes
        100_000
    ];
    let mut group = c.benchmark_group("bloom_filters");

    // Add configuration for better small-set resolution
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(200); // Increased samples for better statistical significance

    for size in sizes {
        let bits_per_key = 10;
        let total_bits = size * bits_per_key;

        // Sample data
        let items: Vec<String> = random_numbers(size, 42)
            .into_iter()
            .map(|n| n.to_string())
            .collect();
        let lookup_items: Vec<String> = random_numbers(size, 43)
            .into_iter()
            .map(|n| n.to_string())
            .collect();

        // Create filters
        let speed_db_bloom = SpeedDbDynamicBloom::new(total_bits as u32, 6);
        let bloom = Bloom::new(total_bits as u32, 6);
        let rocks_bloom = RocksDBLocalBloom::new(total_bits as u32, 6);
        let fast_bloom = BloomFilter::with_num_bits(total_bits)
            .expected_items(size);

        // Benchmark insertions
        group.bench_function(BenchmarkId::new("speeddb_insert", size), |b| {
            b.iter(|| {
                for item in &items {
                    speed_db_bloom.add_hash(item.parse::<u32>().unwrap());
                }
            })
        });

        group.bench_function(BenchmarkId::new("bloom_insert", size), |b| {
            b.iter(|| {
                for item in &items {
                    bloom.add_hash(item.parse::<u32>().unwrap());
                }
            })
        });

        group.bench_function(BenchmarkId::new("rocksdb_insert", size), |b| {
            b.iter(|| {
                for item in &items {
                    let hash = xxh3_128(&item.parse::<u32>().unwrap().to_le_bytes());
                    rocks_bloom.add_hash(hash as u32, (hash >> 32) as u32);
                }
            })
        });

        group.bench_function(BenchmarkId::new("fastbloom_insert", size), |b| {
            b.iter(|| {
                let mut filter = fast_bloom.clone();
                for item in &items {
                    filter.insert(item);
                }
            })
        });

        // Prepare populated filters for lookups
        let mut populated_fast_bloom = fast_bloom.clone();
        for item in &items {
            populated_fast_bloom.insert(item);
            speed_db_bloom.add_hash(item.parse::<u32>().unwrap());
            bloom.add_hash(item.parse::<u32>().unwrap());
            let hash = xxh3_128(&item.parse::<u32>().unwrap().to_le_bytes());
            rocks_bloom.add_hash(hash as u32, (hash >> 32) as u32);
        }

        // Benchmark lookups
        group.bench_function(BenchmarkId::new("speeddb_lookup", size), |b| {
            b.iter(|| {
                for item in &lookup_items {
                    let _ = speed_db_bloom.may_contain(item.parse::<u32>().unwrap());
                }
            })
        });

        group.bench_function(BenchmarkId::new("bloom_lookup", size), |b| {
            b.iter(|| {
                for item in &lookup_items {
                    let _ = bloom.may_contain(item.parse::<u32>().unwrap());
                }
            })
        });

        group.bench_function(BenchmarkId::new("rocksdb_lookup", size), |b| {
            b.iter(|| {
                for item in &lookup_items {
                    let hash = xxh3_128(&item.parse::<u32>().unwrap().to_le_bytes());
                    let _ = rocks_bloom.may_contain(hash as u32, (hash >> 32) as u32);
                }
            })
        });

        group.bench_function(BenchmarkId::new("fastbloom_lookup", size), |b| {
            b.iter(|| {
                for item in &lookup_items {
                    let _ = populated_fast_bloom.contains(item);
                }
            })
        });

        // Benchmark false positives
        let fp_items: Vec<String> = random_numbers(10_000, 44)
            .into_iter()
            .map(|n| n.to_string())
            .collect();

        group.bench_function(BenchmarkId::new("speeddb_fp", size), |b| {
            b.iter(|| {
                let mut fps = 0;
                for item in &fp_items {
                    if speed_db_bloom.may_contain(item.parse::<u32>().unwrap()) {
                        fps += 1;
                    }
                }
                fps
            })
        });

        group.bench_function(BenchmarkId::new("bloom_fp", size), |b| {
            b.iter(|| {
                let mut fps = 0;
                for item in &fp_items {
                    if bloom.may_contain(item.parse::<u32>().unwrap()) {
                        fps += 1;
                    }
                }
                fps
            })
        });

        group.bench_function(BenchmarkId::new("rocksdb_fp", size), |b| {
            b.iter(|| {
                let mut fps = 0;
                for item in &fp_items {
                    let hash = xxh3_128(&item.parse::<u32>().unwrap().to_le_bytes());
                    if rocks_bloom.may_contain(hash as u32, (hash >> 32) as u32) {
                        fps += 1;
                    }
                }
                fps
            })
        });

        group.bench_function(BenchmarkId::new("fastbloom_fp", size), |b| {
            b.iter(|| {
                let mut fps = 0;
                for item in &fp_items {
                    if populated_fast_bloom.contains(item) {
                        fps += 1;
                    }
                }
                fps
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_bloom_filters);
criterion_main!(benches);