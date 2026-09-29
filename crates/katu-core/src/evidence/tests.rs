//! Testes da evidência tipada (DF5/E09-T05).

use super::{ArtifactRef, EvidenceBasis, EvidenceError, Metric, Unit};

fn artifact() -> ArtifactRef {
    ArtifactRef::new("bench/raw.json")
}

fn measured(name: &str, value: f64) -> Result<Metric, EvidenceError> {
    Metric::new(
        name,
        value,
        Unit::Count,
        EvidenceBasis::Measured,
        Some(artifact()),
    )
}

#[test]
fn measured_requires_artifact() {
    let missing = Metric::new("x", 1.0, Unit::Count, EvidenceBasis::Measured, None);
    assert!(matches!(
        missing,
        Err(EvidenceError::MissingArtifact { .. })
    ));
    assert!(measured("x", 1.0).is_ok());
}

#[test]
fn inferred_and_unpriced_need_no_artifact() {
    let inferred = Metric::new("i", 1.0, Unit::Ratio, EvidenceBasis::Inferred, None);
    assert!(inferred.is_ok_and(|metric| !metric.is_publishable()));
}

#[test]
fn unpriced_is_zero_only() {
    let zero = Metric::new("p", 0.0, Unit::Count, EvidenceBasis::Unpriced, None);
    assert!(zero.is_ok());
    let nonzero = Metric::new("p", 1.0, Unit::Count, EvidenceBasis::Unpriced, None);
    assert!(matches!(
        nonzero,
        Err(EvidenceError::UnpricedNonZero { .. })
    ));
}

#[test]
fn publishable_needs_publishable_basis_and_artifact() {
    assert!(measured("m", 3.0).is_ok_and(|metric| metric.is_publishable()));

    let inferred = Metric::new(
        "i",
        3.0,
        Unit::Count,
        EvidenceBasis::Inferred,
        Some(artifact()),
    );
    assert!(inferred.is_ok_and(|metric| !metric.is_publishable()));
}

#[test]
fn sum_preserves_basis_and_artifact() -> Result<(), EvidenceError> {
    let parts = [measured("a", 1.0)?, measured("b", 2.0)?];
    let total = Metric::sum("total", &parts, Unit::Count)?;
    assert!((total.value - 3.0).abs() < f64::EPSILON);
    assert_eq!(total.basis, EvidenceBasis::Measured);
    assert!(total.artifact.is_some());
    Ok(())
}

#[test]
fn sum_rejects_mixed_basis() -> Result<(), EvidenceError> {
    let measured = measured("a", 1.0)?;
    let inferred = Metric::new("b", 2.0, Unit::Count, EvidenceBasis::Inferred, None)?;
    let mixed = Metric::sum("total", &[measured, inferred], Unit::Count);
    assert!(matches!(mixed, Err(EvidenceError::MixedBasis { .. })));
    Ok(())
}

#[test]
fn sum_empty_is_rejected() {
    let empty = Metric::sum("total", &[], Unit::Count);
    assert!(matches!(empty, Err(EvidenceError::Empty)));
}

#[test]
fn round_trips_through_json() -> Result<(), Box<dyn std::error::Error>> {
    let metric = Metric::new(
        "p95",
        120.0,
        Unit::Nanos,
        EvidenceBasis::Measured,
        Some(artifact()),
    )?;
    let encoded = serde_json::to_string(&metric)?;
    let decoded: Metric = serde_json::from_str(&encoded)?;
    assert_eq!(metric, decoded);
    Ok(())
}
