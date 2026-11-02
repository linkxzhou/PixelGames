use super::*;

#[test]
fn runs_counts_and_escapes() {
    assert_eq!(expand_rle("3.2a b", "/r").unwrap(), "...aab");
    assert_eq!(expand_rle("4'3.", "/r").unwrap(), "3333.");
    assert_eq!(expand_rle("12.", "/r").unwrap().len(), 12);
    assert!(expand_rle("0a", "/r").is_err());
    assert!(expand_rle("3", "/r").is_err());
    assert!(expand_rle("999a", "/r").is_err());
    assert!(expand_rle("ab'", "/r").is_err());
}
