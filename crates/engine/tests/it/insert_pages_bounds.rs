//! `layout.pages.insert` refuses an out-of-range `after` or `before` (they must name a page)
//! instead of overflowing, and changes nothing.
use designcraft_engine::Session;
use serde_json::json;

#[test]
fn insert_after_u64_max_is_refused_without_overflow() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages":2})).unwrap();
    let before: Vec<_> = s.doc().unwrap().doc.pages().map(|(_, _, p)| p.id).collect();
    let undo = s.doc().unwrap().history.undo.len();
    for p in [json!({"after": u64::MAX, "count": 2}), json!({"before": u64::MAX, "count": 2})] {
        assert!(s.execute("layout.pages.insert", &p).is_err(), "{p}");
    }
    let after: Vec<_> = s.doc().unwrap().doc.pages().map(|(_, _, p)| p.id).collect();
    assert_eq!(after, before, "no pages added");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo, "no undo step");
    // The last page is still a valid place: the new pages go at the end.
    let out = s.execute("layout.pages.insert", &json!({"after": 1, "count": 2})).unwrap();
    assert_eq!(out["total"], 4);
}
