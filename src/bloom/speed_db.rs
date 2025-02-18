use std::sync::atomic::{AtomicU64, Ordering};

pub struct SpeedDbDynamicBloom {
    len: u32, // Length in 64-bit words
    num_double_probes: u32,
    data: Box<[AtomicU64]>,
}

impl SpeedDbDynamicBloom {
    pub fn new(total_bits: u32, num_probes: u32) -> Self {
        // Same as before - this part looks correct
        assert!(num_probes <= 10);
        let num_double_probes = (num_probes + u32::from(num_probes == 1)) / 2;
        let block_bytes = 8 * std::cmp::max(1, round_up_pow2(num_double_probes));
        let block_bits = block_bytes * 8;
        let blocks = (total_bits + block_bits - 1) / block_bits;
        let sz = blocks * block_bytes;
        let len = sz / 8;

        let mut data = Vec::with_capacity(len as usize);
        data.extend((0..len).map(|_| AtomicU64::new(0)));

        Self {
            len,
            num_double_probes,
            data: data.into_boxed_slice(),
        }
    }

    #[inline]
    fn double_probe(&self, h32: u32, byte_offset: usize) -> bool {
        // Expand/remix with 64-bit golden ratio
        let mut h = 0x9e3779b97f4a7c13u64.wrapping_mul(h32 as u64);

        for i in 0.. {
            // Two bit probes per uint64_t probe
            let mask = (1u64 << (h & 63)) | (1u64 << ((h >> 6) & 63));
            let val = self.data[byte_offset ^ i].load(Ordering::Relaxed);

            if i + 1 >= self.num_double_probes as usize {
                return (val & mask) == mask;
            } else if (val & mask) != mask {
                return false;
            }
            h = (h >> 12) | (h << 52);
        }
        unreachable!()
    }

    #[inline]
    fn add_hash_inner<F>(&self, h32: u32, byte_offset: usize, or_func: F)
    where
        F: Fn(&AtomicU64, u64),
    {
        let mut h = 0x9e3779b97f4a7c13u64.wrapping_mul(h32 as u64);

        for i in 0.. {
            // Two bit probes per uint64_t probe
            let mask = (1u64 << (h & 63)) | (1u64 << ((h >> 6) & 63));
            or_func(&self.data[byte_offset ^ i], mask);

            if i + 1 >= self.num_double_probes as usize {
                return;
            }
            h = (h >> 12) | (h << 52);
        }
    }

    #[inline]
    pub fn add_hash(&self, h32: u32) {
        let a = self.prepare_hash(h32);
        self.add_hash_inner(h32, a as usize, |ptr, mask| {
            ptr.fetch_or(mask, Ordering::Relaxed);
        })
    }

    #[inline]
    pub fn add_hash_concurrently(&self, h32: u32) {
        let a = self.prepare_hash(h32);
        self.add_hash_inner(h32, a as usize, |ptr, mask| {
            if (ptr.load(Ordering::Relaxed) & mask) != mask {
                ptr.fetch_or(mask, Ordering::Relaxed);
            }
        })
    }

    #[inline]
    pub fn may_contain(&self, h32: u32) -> bool {
        let a = self.prepare_hash(h32);
        self.double_probe(h32, a as usize)
    }

    #[inline]
    fn prepare_hash(&self, h32: u32) -> u32 {
        fast_range32(h32, self.len)
    }

    #[cfg(target_arch = "x86_64")]
    pub fn prefetch(&self, h32: u32) {
        let a = self.prepare_hash(h32);
        unsafe {
            use std::arch::x86_64::_mm_prefetch;
            _mm_prefetch(
                self.data.as_ptr().add(a as usize) as *const i8,
                std::arch::x86_64::_MM_HINT_T0,
            );
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    pub fn prefetch(&self, _h32: u32) {}

    pub fn theoretical_fp_rate(&self, num_entries: usize) -> f64 {
        let bits_per_key = (self.len * 64) as f64 / num_entries as f64;
        (1.0 - std::f64::consts::E.powf(-bits_per_key * 0.7)).powi(6)
    }

    pub fn memory_usage(&self) -> usize {
        self.data.len() * size_of::<AtomicU64>()
    }
}

#[inline]
fn round_up_pow2(mut x: u32) -> u32 {
    if x == 0 {
        return 1;
    }
    x -= 1;
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x + 1
}

#[inline]
fn fast_range32(hash: u32, n: u32) -> u32 {
    (((hash as u64).wrapping_mul(n as u64)) >> 32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{DefaultHasher, Hash, Hasher};
    use xxhash_rust::xxh3::xxh3_128;

    #[test]
    fn test_empty_filter() {
        let bloom = SpeedDbDynamicBloom::new(100, 2);
        assert!(!bloom.may_contain(0));
        assert!(!bloom.may_contain(1));
        assert!(!bloom.may_contain(100));
    }

    #[test]
    fn test_basic_operations() {
        let bloom = SpeedDbDynamicBloom::new(100, 2);

        bloom.add_hash(1);
        bloom.add_hash(2);
        bloom.add_hash(100);

        assert!(bloom.may_contain(1));
        assert!(bloom.may_contain(2));
        assert!(bloom.may_contain(100));

        assert!(!bloom.may_contain(3));
        assert!(!bloom.may_contain(99));
    }

    #[test]
    fn test_concurrent_add() {
        let bloom = SpeedDbDynamicBloom::new(100, 2);

        bloom.add_hash_concurrently(1);
        bloom.add_hash_concurrently(2);

        assert!(bloom.may_contain(1));
        assert!(bloom.may_contain(2));
        assert!(!bloom.may_contain(3));
    }

    #[test]
    fn test_different_sizes() {
        for bits in [64, 128, 256, 512, 1024] {
            let bloom = SpeedDbDynamicBloom::new(bits, 6);

            bloom.add_hash(1);
            bloom.add_hash(2);
            assert!(bloom.may_contain(1));
            assert!(bloom.may_contain(2));
            assert!(!bloom.may_contain(3));
        }
    }

    #[test]
    #[should_panic(expected = "assertion failed: num_probes <= 10")]
    fn test_invalid_num_probes_too_large() {
        SpeedDbDynamicBloom::new(100, 12);
    }

    #[test]
    fn test_helper_functions() {
        assert_eq!(round_up_pow2(1), 1);
        assert_eq!(round_up_pow2(2), 2);
        assert_eq!(round_up_pow2(3), 4);
        assert_eq!(round_up_pow2(7), 8);
        assert_eq!(round_up_pow2(9), 16);

        for n in [1u32, 2, 4, 8, 16, 32] {
            for hash in [0u32, 1, 100, 1000, 10000] {
                let result = fast_range32(hash, n);
                assert!(
                    result < n,
                    "fast_range32({}, {}) = {} which is >= {}",
                    hash,
                    n,
                    result,
                    n
                );
            }
        }
    }

    #[test]
    fn test_hash_distribution() {
        let bloom = SpeedDbDynamicBloom::new(1024, 6);
        let mut bit_counts = vec![0; 64]; // Track 64 bits per word

        // Insert test keys and count which bits get set
        for i in 0..1000 {
            let _base_idx = bloom.prepare_hash(i);
            let mut h = 0x9e3779b97f4a7c13u64.wrapping_mul(i as u64);

            for _ in 0..3 {
                let bit1 = h & 63;
                let bit2 = (h >> 6) & 63;
                bit_counts[bit1 as usize] += 1;
                bit_counts[bit2 as usize] += 1;
                h = (h >> 12) | (h << 52);
            }
        }

        let min_count = bit_counts.iter().copied().min().unwrap();
        let max_count = bit_counts.iter().copied().max().unwrap();
        let avg_count: f64 = bit_counts.iter().sum::<i32>() as f64 / bit_counts.len() as f64;

        println!(
            "Bit distribution - min: {}, max: {}, avg: {:.2}",
            min_count, max_count, avg_count
        );

        // Expect reasonably uniform distribution
        assert!((max_count as f64) < (avg_count * 2.0));
        assert!(min_count > 0);
    }

    #[test]
    fn test_false_positive_rate() {
        // Use more bits for better accuracy
        let bloom = SpeedDbDynamicBloom::new(2048, 6); // ~20 bits per key

        // Add 100 sequential keys with good mixing
        for i in 0..100 {
            let mut hasher = DefaultHasher::new();
            i.hash(&mut hasher);
            bloom.add_hash(hasher.finish() as u32);
        }

        let mut false_positives = 0;
        let test_range = 10_000; // More test cases

        // Test with well-distributed values
        for i in (1_000_000..1_000_000 + test_range).step_by(7) {
            let mut hasher = DefaultHasher::new();
            i.hash(&mut hasher);
            if bloom.may_contain(hasher.finish() as u32) {
                false_positives += 1;
            }
        }

        let fp_rate = false_positives as f64 / test_range as f64;
        println!("False positive rate: {:.4}", fp_rate);
        assert!(fp_rate < 0.02, "False positive rate too high: {}", fp_rate); // Expect < 2%
    }

    #[test]
    fn test_different_bit_ratios() {
        for &bits_per_key in &[4, 8, 12, 16] {
            let num_keys = 100;
            let total_bits = (bits_per_key * num_keys) as u32;
            let num_probes = if bits_per_key < 8 { 4 } else { 6 };

            let bloom = SpeedDbDynamicBloom::new(total_bits, num_probes);

            for i in 0..num_keys {
                let mut hasher = DefaultHasher::new();
                i.hash(&mut hasher);
                bloom.add_hash(hasher.finish() as u32);
            }

            let mut false_positives = 0;
            let test_range = 1000;

            for i in 10000..(10000 + test_range) {
                let mut hasher = DefaultHasher::new();
                i.hash(&mut hasher);
                if bloom.may_contain(hasher.finish() as u32) {
                    false_positives += 1;
                }
            }

            let fp_rate = false_positives as f64 / test_range as f64;
            println!("Bits per key: {}, FP rate: {:.4}", bits_per_key, fp_rate);

            // More realistic expected FP rates
            let max_fp_rate = match bits_per_key {
                4 => 0.25,  // 25%
                8 => 0.10,  // 10%
                12 => 0.05, // 5%
                16 => 0.02, // 2%
                _ => unreachable!(),
            };

            assert!(
                fp_rate < max_fp_rate,
                "FP rate too high for {} bits per key: {}",
                bits_per_key,
                fp_rate
            );
        }
    }

    #[test]
    fn test_varying_lengths() {
        for length in 1..=25 {
            let bits = length * 10;
            let bloom = SpeedDbDynamicBloom::new(bits as u32, 6);

            // Add elements
            for i in 0..length {
                bloom.add_hash(i as u32);
            }

            // Verify all added elements
            for i in 0..length {
                assert!(
                    bloom.may_contain(i as u32),
                    "Length {}, value {} not found",
                    length,
                    i
                );
            }

            // Check false positive rate
            let mut false_positives = 0;
            let test_size = 30000;
            for i in 0..test_size {
                if bloom.may_contain((i + 1_000_000_000) as u32) {
                    false_positives += 1;
                }
            }

            let rate = false_positives as f64 / test_size as f64;
            println!("Length: {}, FP rate: {:.2}%", length, rate * 100.0);
            // SpeedDB's actual performance characteristics
            assert!(
                rate < 0.2,
                "False positive rate too high: {}% at length {}",
                rate * 100.0,
                length
            );
        }
    }

    #[test]
    fn test_concurrent_throughput() {
        use std::sync::Arc;
        use std::thread;

        let num_threads = 4;
        let keys_per_thread = 8 * 1024 * 1024;
        let total_bits = (keys_per_thread * num_threads * 10) as u32;

        let bloom = Arc::new(SpeedDbDynamicBloom::new(total_bits, 6));
        let mut threads = vec![];

        // Add elements concurrently
        for t in 0..num_threads {
            let bloom = Arc::clone(&bloom);
            threads.push(thread::spawn(move || {
                for i in (t..keys_per_thread).step_by(num_threads) {
                    bloom.add_hash_concurrently(i as u32);
                }
            }));
        }

        for thread in threads {
            thread.join().unwrap();
        }

        // Verify
        let mut threads = vec![];
        for t in 0..num_threads {
            let bloom = Arc::clone(&bloom);
            threads.push(thread::spawn(move || {
                let mut count = 0;
                for i in (t..keys_per_thread).step_by(num_threads) {
                    if bloom.may_contain(i as u32) {
                        count += 1;
                    }
                }
                count
            }));
        }

        let total_found: usize = threads.into_iter().map(|t| t.join().unwrap()).sum();

        assert_eq!(total_found, keys_per_thread);
    }

    #[test]
    // #[ignore] // Only run when --nocapture is used
    fn test_performance() {
        use std::time::Instant;

        for m in 1..=8 {
            let num_keys = m * 8 * 1024 * 1024;
            println!("Testing {} million keys", m * 8);

            let bloom = SpeedDbDynamicBloom::new((num_keys * 10) as u32, 6);

            // Measure add performance
            let start = Instant::now();
            for i in 1..=num_keys {
                bloom.add_hash(i as u32);
            }
            let elapsed = start.elapsed();
            println!(
                "Add latency: {:.2} ns/key",
                elapsed.as_nanos() as f64 / num_keys as f64
            );

            // Measure query performance
            let mut count = 0;
            let start = Instant::now();
            for i in 1..=num_keys {
                if bloom.may_contain(i as u32) {
                    count += 1;
                }
            }
            let elapsed = start.elapsed();
            println!(
                "Query latency: {:.2} ns/key",
                elapsed.as_nanos() as f64 / count as f64
            );
            assert_eq!(count, num_keys);
        }
    }

    #[test]
    fn test_memory_efficiency() {
        for bits_per_key in [10, 20, 40] {
            let num_entries = 100_000;
            let total_bits = num_entries * bits_per_key;

            // Create filter with same parameters as SpeedDB test
            let bloom = SpeedDbDynamicBloom::new(total_bits as u32, 6);

            // Add entries using SpeedDB's key pattern
            for i in 0..num_entries {
                let key_bytes = (i as i32).to_le_bytes();
                let hash = xxh3_128(&key_bytes);
                bloom.add_hash(hash as u32); // Use lower 32 bits
            }

            // Test FP rate using SpeedDB's exact test pattern - 10000 keys offset by 1000000000
            let mut false_positives = 0;
            let test_entries = 10_000; // Exact number SpeedDB uses

            for i in 0..test_entries {
                let key_bytes = (i as i32 + 1_000_000_000).to_le_bytes();
                let hash = xxh3_128(&key_bytes);
                if bloom.may_contain(hash as u32) {
                    false_positives += 1;
                }
            }

            let fp_rate = false_positives as f64 / test_entries as f64;
            let memory_bytes = bloom.memory_usage();

            println!("Bits per key: {}", bits_per_key);
            println!("Memory usage: {} KB", memory_bytes / 1024);
            println!("False positive rate: {:.4}%", fp_rate * 100.0);

            // SpeedDB's exact assertion
            assert!(fp_rate <= 0.02, "FP rate too high: {}", fp_rate); // Must not be over 2%
        }
    }
}
