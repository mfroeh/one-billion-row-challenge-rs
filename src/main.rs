use std::{
    collections::HashMap,
    fs::File,
    io::{IoSliceMut, Read, Write},
    mem::ManuallyDrop,
    os::unix::fs::MetadataExt,
};

#[derive(Default)]
struct Statistics {
    min: f64,
    sum: f64,
    max: f64,
    count: usize,
}

impl Statistics {
    fn add(&mut self, measurement: f64) {
        self.min = self.min.min(measurement);
        self.max = self.max.max(measurement);
        self.sum += measurement;
        self.count += 1;
    }

    fn complete(self) -> (f64, f64, f64) {
        (self.min, (self.sum / self.count as f64), self.max)
    }
}

fn main() {
    let mut file = File::open("measurements.txt").unwrap();
    let size = file.metadata().unwrap().size() as usize;

    let chunk_size = 2usize.pow(14);
    let chunk_count = size.div_ceil(chunk_size);
    let mut buf = vec![0; chunk_size * chunk_count];

    let mut iovecs: Vec<_> = buf
        .chunks_exact_mut(chunk_size)
        .map(|c| IoSliceMut::new(c))
        .collect();

    let mut read = 0;
    while read < size {
        // Assumption: Chunks are always fully read, or not read at all.
        // Apart from the last chunk, which may only be partially full, due to the file ending.
        read += file
            .read_vectored(&mut iovecs[read / chunk_size..])
            .unwrap();
    }
    let file = unsafe { String::from_raw_parts(buf.as_mut_ptr(), size, buf.capacity()) };
    let _ = ManuallyDrop::new(buf);

    let mut cities: HashMap<&str, Statistics> = HashMap::new();
    for line in file.lines() {
        let (city, temp) = line.split_once(";").unwrap();
        let measurement: f64 = temp.parse().unwrap();
        cities.entry(city).or_default().add(measurement);
    }

    write!(std::io::stdout(), "{{").unwrap();
    cities.into_iter().enumerate().for_each(|(i, (city, s))| {
        let (min, avg, max) = s.complete();
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
