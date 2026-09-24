//! Per-key source merging, exercised internally until semantic override validation ships.
#[cfg(test)]
pub fn merge_per_key(base: toml::Value, override_: &toml::Value) -> toml::Value {
    match (base, override_) {
        (toml::Value::Table(mut base_table), toml::Value::Table(override_table)) => {
            for (key, value) in override_table {
                let merged = match base_table.remove(key) {
                    Some(existing) => merge_per_key(existing, value),
                    None => value.clone(),
                };
                base_table.insert(key.clone(), merged);
            }
            toml::Value::Table(base_table)
        }
        (_, value) => value.clone(),
    }
}
