//! Two tests that pass and one that is ignored. `tests/live.sh` requires the
//! script to read exactly that from this cargo's output.

#[test]
fn adds() {
    assert_eq!(prove_tests_ran_live::one() + 1, 2);
}

#[test]
fn is_one() {
    assert_eq!(prove_tests_ran_live::one(), 1);
}

#[test]
#[ignore = "counted as ignored by tests/live.sh"]
fn ignored() {
    assert_eq!(prove_tests_ran_live::one(), 2);
}
