use std::{
    collections::HashMap,
    ffi::c_void,
    fs::File,
    io::{Error, Write},
    ops::Range,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    panic, slice, thread,
};

use fasthash::RandomState;
use libc::{MADV_HUGEPAGE, MAP_FAILED, MAP_PRIVATE, PROT_READ};

#[derive(Default)]
struct Statistics {
    min: i32,
    sum: i32,
    max: i32,
    count: usize,
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

fn main() {
    let file = File::open("measurements.txt").unwrap();
    // We get rid off the trailing LF char (0x0a == '\n' as u8), to avoid the split() yielding an empty slice,
    // and thus get rid off one branch (this branch is roughly 100ms, since this is such a tight loop).
    // Useful to see if a file has a trailing LF: `tail measurements.txt | xxd`
    let size = file.metadata().unwrap().size() as usize - 1;

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

    let mut chunks: Vec<Range<usize>> = Vec::new();
    let mut start = 0;
    for _ in 0..chunk_count {
        let exact_end = string.ceil_char_boundary(start + chunk_size);
        let end = string[exact_end..]
            .find('\n')
            .map(|e| e + exact_end)
            // it is always possible that exact_end hits the very last line
            .unwrap_or(size);
        chunks.push(start..end);
        start = end + 1;
    }
    // last chunk gets the remainder of the work
    chunks.last_mut().unwrap().end = size;

    let merged = thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(|| {
                    let s = RandomState::new();
                    let mut cities: HashMap<
                        &[u8],
                        Statistics,
                        RandomState<fasthash::city::Hash64>,
                    > = HashMap::with_capacity_and_hasher(400, s);
                    for line in string[chunk].as_bytes().split(|&b| b == '\n' as u8) {
                        let len = line.len();
                        // Luanda;6.0
                        let split_at = if line[len - 4] == ';' as u8 {
                            len - 4
                        } else if line[len - 5] == ';' as u8 {
                            len - 5
                        } else {
                            len - 6
                        };

                        let temp = parse_temperature(&line[split_at + 1..]);
                        cities.entry(&line[..split_at]).or_default().add(temp);
                    }
                    cities
                })
            })
            .collect();

        let mut merged: Option<HashMap<&[u8], Statistics, RandomState<fasthash::city::Hash64>>> =
            None;
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

    write!(std::io::stdout(), "{{").unwrap();
    merged
        .unwrap()
        .into_iter()
        .enumerate()
        .for_each(|(i, (city, s))| {
            let (min, avg, max) = s.complete();
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
