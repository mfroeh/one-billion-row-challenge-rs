use std::{
    ffi::c_void,
    fs::File,
    io::{Error, Write},
    ops::Range,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    panic, slice, thread,
};

use libc::{MADV_HUGEPAGE, MAP_FAILED, MAP_PRIVATE, PROT_READ};
use ptr_hash::{
    FastPtrHash, PtrHash, PtrHashParams,
    bucket_fn::{self},
    hash::FastIntHash,
};
use rustc_hash::{FxBuildHasher, FxHashSet};

#[derive(Default, Clone, Copy)]
struct Statistics {
    min: i32,
    sum: i32,
    max: i32,
    count: u32,
}

impl Statistics {
    fn add(&mut self, temp: i32) {
        self.min = self.min.min(temp);
        self.max = self.max.max(temp);
        self.sum += temp;
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
    let neg = from[0] == '-' as u8;
    if neg {
        let mut n: i32 = (from[1] - '0' as u8) as i32;
        n *= 10;
        if from[2] == '.' as u8 {
            n += (from[3] - '0' as u8) as i32;
        } else {
            n += (from[2] - '0' as u8) as i32;
            n *= 10;
            n += (from[4] - '0' as u8) as i32;
        }
        -n
    } else {
        let mut n: i32 = (from[0] - '0' as u8) as i32;
        n *= 10;
        if from[1] == '.' as u8 {
            n += (from[2] - '0' as u8) as i32;
        } else {
            n += (from[1] - '0' as u8) as i32;
            n *= 10;
            n += (from[3] - '0' as u8) as i32;
        }
        n
    }
}

#[inline]
fn position_semi(line: &[u8]) -> usize {
    let len = line.len();
    // Luanda;6.0
    if line[len - 4] == ';' as u8 {
        len - 4
    } else if line[len - 5] == ';' as u8 {
        len - 5
    } else {
        len - 6
    }
}

fn process_chunk(
    chunk: &[u8],
    phf: &PtrHash<&[u8], bucket_fn::Linear, Vec<u32>, FastIntHash, Vec<u8>, true, false>,
) -> Vec<Statistics> {
    let mut cities: Vec<Statistics> = vec![Statistics::default(); phf.max_index()];
    let mut line_start = 0;
    for line_end in memchr::memchr_iter('\n' as u8, chunk) {
        let line = &chunk[line_start..line_end];
        line_start = line_end + 1;
        let split_at = position_semi(line);
        let temp = parse_temperature(&line[split_at + 1..]);
        let city = &line[..split_at];
        cities[phf.index(&city)].add(temp);
    }
    cities
}

fn write_result(
    result: Vec<Statistics>,
    cities: Vec<&[u8]>,
    phf: PtrHash<&[u8], bucket_fn::Linear, Vec<u32>, FastIntHash, Vec<u8>, true, false>,
) {
    write!(std::io::stdout(), "{{").unwrap();
    cities.into_iter().enumerate().for_each(|(i, city)| {
        let (min, avg, max) = result[phf.index(&city)].complete();
        let city = unsafe { str::from_utf8_unchecked(city) };
        write!(
            std::io::stdout(),
            "{}\"{city}\": {{\"min\": {min:.1}, \"avg\": {avg:.1}, \"max\": {max:.1}}}",
            if i != 0 { "," } else { "" }
        )
        .unwrap();
    });
    write!(std::io::stdout(), "}}").unwrap();
    std::io::stdout().flush().unwrap();
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
        let end = exact_end
            + memchr::memchr('\n' as u8, string[exact_end..].as_bytes()).unwrap_or(size - 1);
        chunks.push(start..end + 1);
        start = end + 1;
    }
    // last chunk gets the remainder of the work
    chunks.last_mut().unwrap().end = size;

    let mut cities = FxHashSet::with_capacity_and_hasher(1000, FxBuildHasher);
    let mut line_start = 0;
    let string: &[u8] = string.as_bytes();
    for line_end in memchr::memchr_iter('\n' as u8, string).take(10000) {
        let line = &string[line_start..line_end];
        line_start = line_end + 1;

        let city_end = position_semi(line);
        cities.insert(&line[..city_end]);
    }
    let cities = cities.into_iter().collect::<Vec<_>>();

    let phf: PtrHash<&[u8], bucket_fn::Linear, Vec<u32>, FastIntHash, Vec<u8>, true, false> =
        FastPtrHash::new(&cities, PtrHashParams::default_fast());

    let merged = thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| scope.spawn(|| process_chunk(&string[chunk], &phf)))
            .collect();

        let mut merged = vec![Statistics::default(); phf.max_index()];
        for h in handles {
            let chunk_result = h.join().unwrap();
            for i in 0..phf.max_index() {
                merged[i].merge_with(chunk_result[i]);
            }
        }
        merged
    });

    write_result(merged, cities, phf);
}
