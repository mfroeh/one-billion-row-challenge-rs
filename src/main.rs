#![feature(default_field_values)]
use std::{
    ffi::c_void,
    fs::File,
    io::Error,
    ops::Range,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    panic, slice, thread,
};

use libc::{MADV_HUGEPAGE, MAP_FAILED, MAP_PRIVATE, PROT_READ};
use rustc_hash::{FxBuildHasher, FxHashMap};

#[derive(Default)]
struct Statistics {
    min: i32 = i32::MAX,
    max: i32 = i32::MIN,
    sum: i64,
    count: usize,
}

impl Statistics {
    fn add(&mut self, temp: i32) {
        self.min = self.min.min(temp);
        self.max = self.max.max(temp);
        self.sum += temp as i64;
        self.count += 1;
    }

    fn complete(self) -> (f64, f64, f64) {
        (
            self.min as f64 / 10.0,
            (self.sum as f64 / 10.0 / self.count as f64),
            self.max as f64 / 10.0,
        )
    }

    fn merge_with(&mut self, other: Statistics) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum += other.sum;
        self.count += other.count;
    }
}

const HUGE_PAGE_SIZE: usize = 2usize.pow(21);

fn parse_temperature(from: &[u8]) -> i32 {
    let neg = from[0] == b'-';
    if neg {
        let mut n: i32 = (from[1] - b'0') as i32;
        n *= 10;
        if from[2] == b'.' {
            n += (from[3] - b'0') as i32;
        } else {
            n += (from[2] - b'0') as i32;
            n *= 10;
            n += (from[4] - b'0') as i32;
        }
        -n
    } else {
        let mut n: i32 = (from[0] - b'0') as i32;
        n *= 10;
        if from[1] == b'.' {
            n += (from[2] - b'0') as i32;
        } else {
            n += (from[1] - b'0') as i32;
            n *= 10;
            n += (from[3] - b'0') as i32;
        }
        n
    }
}

fn process_chunk(chunk: &[u8]) -> FxHashMap<&[u8], Statistics> {
    let mut cities: FxHashMap<&[u8], Statistics> =
        FxHashMap::with_capacity_and_hasher(400, FxBuildHasher);

    let mut line_start = 0;
    for line_end in memchr::memchr_iter(b'\n', chunk) {
        let line = &chunk[line_start..line_end];
        line_start = line_end + 1;

        let len = line.len();
        // Luanda;6.0
        let split_at = if line[len - 4] == b';' {
            len - 4
        } else if line[len - 5] == b';' {
            len - 5
        } else {
            len - 6
        };

        let temp = parse_temperature(&line[split_at + 1..]);
        cities.entry(&line[..split_at]).or_default().add(temp);
    }
    cities
}

// Java's Math.Round, which rounds towards positive infinity.
fn round(v: f64) -> f64 {
    (v + 0.5).floor()
}

fn write_result(result: FxHashMap<&[u8], Statistics>) {
    print!("{{");
    for (i, (city, s)) in result.into_iter().enumerate() {
        let city = unsafe { str::from_utf8_unchecked(city) };
        let (min, mean, max) = s.complete();
        let (min, mean, max) = (
            round(min * 10.0) / 10.0,
            round(mean * 10.0) / 10.0,
            round(max * 10.0) / 10.0,
        );
        if i == 0 {
            print!("{city}={min:.1}/{mean:.1}/{max:.1}");
        } else {
            print!(", {city}={min:.1}/{mean:.1}/{max:.1}");
        }
    }
    print!("}}");
}

fn main() {
    let file = File::open("measurements.txt").unwrap();
    let size = file.metadata().unwrap().size() as usize;

    let map_addr = unsafe {
        let addr = libc::mmap(
            std::ptr::null_mut::<c_void>(),
            size,
            PROT_READ,
            // MAP_HUGETLB does not seem to be supported on WSL Ubuntu 26.04
            MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        );
        if addr == MAP_FAILED {
            panic!("mmap: {}", Error::last_os_error());
        }
        // But Transparent Huge Pages (THP) are.
        // This will cause the kernel to allocate huge pages for the region specified by addr and size.
        // Note that this requires for addr to be huge page size aligned, which experimentally seems to always be the case if you give mmap a null ptr.
        assert_eq!(addr.addr() % HUGE_PAGE_SIZE, 0);
        if libc::madvise(addr, size, MADV_HUGEPAGE) == -1 {
            panic!("madvise: {}", Error::last_os_error());
        };
        addr.cast::<u8>()
    };
    let mapped_bytes = unsafe { slice::from_raw_parts(map_addr, size) };
    let string = unsafe { str::from_utf8_unchecked(mapped_bytes) };

    let chunk_count: usize = thread::available_parallelism().unwrap().get();
    let chunk_size = size / chunk_count;

    let mut chunks: Vec<Range<usize>> = Vec::with_capacity(chunk_count);
    let mut start = 0;
    for _ in 0..chunk_count {
        let exact_end = string.ceil_char_boundary(start + chunk_size);
        // For the last chunk, it is possible that exact_end == size - 1.
        let end =
            exact_end + memchr::memchr(b'\n', &string.as_bytes()[exact_end..]).unwrap_or(size - 1);
        chunks.push(start..end + 1);
        start = end + 1;
    }
    // last chunk gets the remainder of the work
    chunks.last_mut().unwrap().end = size;

    let merged = thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let chunk = string[chunk].as_bytes();
                scope.spawn(|| process_chunk(chunk))
            })
            .collect();

        let mut merged: Option<FxHashMap<&[u8], Statistics>> = None;
        for h in handles {
            let chunk_result = h.join().unwrap();
            if let Some(merged) = merged.as_mut() {
                for (k, v) in chunk_result {
                    merged.entry(k).or_default().merge_with(v);
                }
            } else {
                merged = Some(chunk_result)
            }
        }
        merged
    });

    write_result(merged.unwrap());
}
