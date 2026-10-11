use super::*;
#[test]
fn signing_matches_node_java_form() {
    assert_eq!(
        encode("汉 字+~!'()*"),
        "%E6%B1%89+%E5%AD%97%2B%7E%21%27%28%29*"
    );
    let p = BTreeMap::from([
        ("a".into(), "1".into()),
        ("timestamp".into(), "1700000000".into()),
        ("z".into(), "汉 字&+".into()),
    ]);
    let r = signed(p, "synthetic-key");
    let tech = format!("sdk|{}", hash(&hash("sdk_synthetic-key_1700000000")));
    assert_eq!(
        r["sign"],
        hash(&format!(
            "a=1&tech_version={tech}&timestamp=1700000000&z=汉 字&+_synthetic-key"
        ))
    );
}
