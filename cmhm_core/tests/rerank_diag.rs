#![cfg(feature = "onnx")]
//! B13 instrument diagnostic (not an eval artifact): positive/negative controls
//! for the rerank probe. If the exact-match control fails, the instrument (not
//! the corpus) is broken and probe numbers are void.

#[test]
#[ignore]
fn rerank_diag() {
    let mut rr = fastembed::TextRerank::try_new(fastembed::RerankInitOptions::new(
        fastembed::RerankerModel::JINARerankerV1TurboEn,
    ))
    .expect("reranker init");
    let q = "sextant reading took 12 minutes under overcast";
    let docs = vec![
        "sextant reading took 12 minutes under overcast",  // exact match
        "sextant reading happened at noon, six minutes",   // near-paraphrase
        "the payroll system desynchronized badges in may", // unrelated
    ];
    let r = rr.rerank(q, &docs, false, None).unwrap();
    println!("-- jina-reranker-v1-turbo-en --");
    for x in &r {
        println!("index={} score={:.5} doc={:?}", x.index, x.score, docs[x.index]);
    }

    let mut bge = fastembed::TextRerank::try_new(fastembed::RerankInitOptions::new(
        fastembed::RerankerModel::BGERerankerBase,
    ))
    .expect("bge reranker init");
    let r2 = bge.rerank(q, &docs, false, None).unwrap();
    println!("-- BAAI/bge-reranker-base --");
    for x in &r2 {
        println!("index={} score={:.5} doc={:?}", x.index, x.score, docs[x.index]);
    }
    let top = r2[0].index;
    println!("bge control top-1 index = {top} (expect 0)");
}
