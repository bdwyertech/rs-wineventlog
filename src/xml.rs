use roxmltree::Document;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

pub fn parse_to_json(xml: &str) -> Option<JsonValue> {
    let doc = Document::parse(xml).ok()?;
    let root = doc
        .root_element()
        .first_element_child()
        .unwrap_or(doc.root_element());
    Some(element_to_json(root))
}

fn element_to_json(node: roxmltree::Node) -> JsonValue {
    let mut map = serde_json::Map::new();

    for attr in node.attributes() {
        map.insert(
            format!("@{}", attr.name()),
            JsonValue::String(attr.value().to_string()),
        );
    }

    let mut children: HashMap<String, Vec<JsonValue>> = HashMap::new();
    for child in node.children().filter(|n| n.is_element()) {
        let name = child.tag_name().name().to_string();
        children
            .entry(name)
            .or_default()
            .push(element_to_json(child));
    }

    for (k, v) in children {
        if v.len() == 1 {
            map.insert(k, v.into_iter().next().unwrap());
        } else {
            map.insert(k, JsonValue::Array(v));
        }
    }

    if node.children().filter(|n| n.is_element()).count() == 0 {
        let text = node.text().unwrap_or("").trim();
        if !text.is_empty() {
            return JsonValue::String(text.to_string());
        }
    }

    JsonValue::Object(map)
}
