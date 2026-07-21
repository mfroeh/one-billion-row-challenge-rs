# One Billion Row Challenge
CPU: 9950X3D 32 Hardware Threads @ 4.3/5.7 GHz, RAM: 32GB DDR5 6000 MHz 

## Rules of the original challenge
1. No external library dependencies may be used
2. Implementations must be provided as a single source file
3. The computation must happen at application runtime, i.e. you cannot process the measurements file at build time (for instance, when using GraalVM) and just bake the result into the binary
4. Input value ranges are as follows:
  * Station name: non null UTF-8 string of min length 1 character and max length 100 bytes, containing neither ; nor \n characters. (i.e. this could be 100 one-byte characters, or 50 two-byte characters, etc.)
  * Temperature value: non null double between -99.9 (inclusive) and 99.9 (inclusive), always with one fractional digit
5. There is a maximum of 10,000 unique station names
6. Line endings in the file are \n characters on all platforms
7. Implementations must not rely on specifics of a given data set, e.g. any valid station name as per the constraints above and any data distribution (number of measurements per station) must be supported
8. The rounding of output values must be done using the semantics of IEEE 754 rounding-direction "roundTowardPositive"

### Measurements
1. Naive: `40.18s user 13.71s system 90% cpu 59.279 total`
2. Assume UTF-8: `38.65s user 9.65s system 99% cpu 48.406 total`
3. Vectored Read: `38.26s user 10.21s system 99% cpu 48.581 total`
4. mmap: `37.54s user 0.81s system 99% cpu 38.383 total`
5. Parallelize: `71.37s user 3.27s system 2805% cpu 2.661 total`
6. CityHash64: `76.58s user 0.67s system 2937% cpu 2.630 total`
7. Transparent Huge Pages: `76.30s user 0.11s system 3109% cpu 2.457 total`
8. f64 -> i32 and manual parsing: `64.16s user 0.17s system 3033% cpu 2.121 total`
9. rsplit_once: `61.57s user 0.20s system 3020% cpu 2.045 total`
10. `&[u8]` instead of `&str`: `41.76s user 0.10s system 3036% cpu 1.379 total`
