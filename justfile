generate rows:
    cargo r --release --bin generate {{rows}}

run:
    cargo r --release --bin one-billion-row-challenge > out

verify:
    cargo r --bin verify
