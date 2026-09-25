//! Verify a projection matrix binary loads and behaves sanely.
//!
//! Usage:
//!   cargo run --example verify_projection -- data/projections/minilm-l6-v2-int8.bin

use cmhm_core::projection::Projection;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} <path-to-projection.bin>", args[0]);
        std::process::exit(1);
    }
    let path = &args[1];
    let bytes = fs::read(path).expect("read file");
    println!("[verify] file size: {} bytes", bytes.len());

    let proj = Projection::from_bytes(&bytes).expect("load projection");
    println!("[verify] input_dim:  {}", proj.input_dim);
    println!("[verify] matrix len: {} (expected {} x {} = {})",
             proj.matrix.len(), 128usize, proj.input_dim, 128 * proj.input_dim);
    println!("[verify] bias len:   {}", proj.bias.len());

    // Frobenius norm of matrix — should be non-zero and O(sqrt(128))
    let mag: f32 = proj.matrix.iter().map(|x| x * x).sum::<f32>().sqrt();
    println!("[verify] matrix frobenius: {:.4}", mag);

    // Apply to a test input: one-hot on dim 0
    let mut test_input = vec![0.0f32; proj.input_dim];
    test_input[0] = 1.0;
    let out = proj.project(&test_input).expect("project");
    let out_norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    println!("[verify] output norm after normalize: {:.6}", out_norm);

    // Assert output is unit norm (project() normalizes)
    assert!(
        (out_norm - 1.0).abs() < 1e-4,
        "project() must return unit-norm output, got {}",
        out_norm
    );

    // Assert not degenerate: at least 10% of output components non-zero
    let nonzero = out.iter().filter(|x| x.abs() > 1e-6).count();
    println!("[verify] non-zero components: {} / 128", nonzero);
    assert!(nonzero >= 12, "output too sparse ({} non-zero) — matrix may be broken", nonzero);

    println!("[verify] OK");
}
