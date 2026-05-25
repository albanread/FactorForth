//! compress-image — one-shot tool that compresses the Factor bootstrap
//! image with zstd for distribution.
//!
//! Usage:
//!   compress-image <input.image> [output.image.zst]
//!
//! Defaults to writing `<input>.zst` next to the input.  Level 19 (high
//! compression, slow encode) is the right trade-off here: we encode
//! once at release time and ship the result to every user, so spending
//! a minute on the encoder to save tens of megabytes per download is
//! a clear win.  Decoding is symmetric in zstd — fast at every level.

use std::env;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: {} <input.image> [output.image.zst]", args[0]);
        std::process::exit(2);
    }
    let input = PathBuf::from(&args[1]);
    let output = if args.len() == 3 {
        PathBuf::from(&args[2])
    } else {
        let mut o = input.clone();
        let name = o.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        o.set_file_name(format!("{name}.zst"));
        o
    };

    let raw_meta = std::fs::metadata(&input)?;
    let raw_size = raw_meta.len();
    println!("compressing {} ({} bytes) → {} at zstd level 19",
        input.display(), raw_size, output.display());

    let t0 = Instant::now();
    let mut src = File::open(&input)?;
    let dst = File::create(&output)?;
    // Level 19 = high compression, suitable for ship-once / decode-many.
    // The Factor image is mostly tagged-pointer data + bytecode arrays;
    // we see ~4× ratio at this level in practice.
    let mut enc = zstd::Encoder::new(dst, 19)?;
    io::copy(&mut src, &mut enc)?;
    enc.finish()?;

    let out_size = std::fs::metadata(&output)?.len();
    let elapsed = t0.elapsed();
    let ratio = raw_size as f64 / out_size.max(1) as f64;
    println!("done in {:.2}s — {} bytes ({:.2}× smaller)",
        elapsed.as_secs_f64(), out_size, ratio);
    Ok(())
}

