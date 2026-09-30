//! Testes do filtro de Bloom (sem falsos negativos; rejeita termos ausentes).

use super::Bloom;

#[test]
fn has_no_false_negatives() {
    let terms = ["parser", "toon", "densidade", "audit", "varint"];
    let bloom = Bloom::from_terms(terms.iter().copied());
    for term in terms {
        assert!(bloom.might_contain(term), "falso negativo para {term}");
    }
}

#[test]
fn rejects_absent_term() {
    let bloom = Bloom::from_terms(["parser", "toon"].into_iter());
    assert!(!bloom.might_contain("zzz_inexistente_999"));
}
