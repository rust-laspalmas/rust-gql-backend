use anyhow::Context;
use async_graphql::parser::parse_schema;
use async_graphql::parser::types::{
    BaseType, ServiceDocument, Type, TypeKind, TypeSystemDefinition,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Directives that any async-graphql SDL always declares; they are part of the
/// GraphQL spec, not of our contract, so the diff ignores them.
const BUILTIN_DIRECTIVES: [&str; 4] = ["skip", "include", "deprecated", "specifiedBy"];

/// A schema reduced to the pieces the wire contract depends on: object types
/// with their fields (name → rendered type), enums with their values, plus
/// scalar and directive names for informational reporting. Descriptions, the
/// `schema {}` block and argument defaults are intentionally dropped.
#[derive(Default)]
struct Contract {
    objects: BTreeMap<String, BTreeMap<String, String>>,
    enums: BTreeMap<String, BTreeSet<String>>,
    scalars: BTreeSet<String>,
    directives: BTreeSet<String>,
}

/// Outcome of comparing the Node contract against the emitted Rust SDL.
pub struct Report {
    /// Changes that would break the existing SPA (removed/changed type or field).
    pub breaking: Vec<String>,
    /// Additive or non-breaking observations.
    pub info: Vec<String>,
    /// Set when the committed `schema.graphql` is missing or does not match the
    /// freshly emitted SDL.
    pub stale: Option<String>,
}

/// Write the freshly built schema SDL to `out`.
pub fn emit_schema(out: &Path) -> anyhow::Result<()> {
    let sdl = api::build_schema().sdl();
    std::fs::write(out, sdl).with_context(|| format!("writing SDL to {}", out.display()))?;
    Ok(())
}

/// Compare the emitted Rust SDL against the Node `.gql` contract.
pub fn diff(out: &Path, node_dir: &Path) -> anyhow::Result<Report> {
    let emitted = api::build_schema().sdl();

    let node_sdl = read_node_sdl(node_dir)
        .with_context(|| format!("reading Node .gql files under {}", node_dir.display()))?;

    let node = normalize(parse_schema(&node_sdl).context("parsing Node .gql contract")?);
    let rust = normalize(parse_schema(&emitted).context("parsing emitted Rust SDL")?);

    let mut report = compare(&node, &rust);
    report.stale = staleness(out, &emitted);
    Ok(report)
}

/// Recursively concatenate every `*.gql` file under `dir` (skipping
/// `*.gql.disabled`, whose extension is not `gql`).
fn read_node_sdl(dir: &Path) -> anyhow::Result<String> {
    let mut buffer = String::new();
    collect_gql(dir, &mut buffer)?;
    anyhow::ensure!(
        !buffer.trim().is_empty(),
        "no .gql files found under {}",
        dir.display()
    );
    Ok(buffer)
}

fn collect_gql(dir: &Path, buffer: &mut String) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_gql(&path, buffer)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("gql") {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            buffer.push_str(&content);
            buffer.push('\n');
        }
    }
    Ok(())
}

fn normalize(document: ServiceDocument) -> Contract {
    let mut contract = Contract::default();

    for definition in document.definitions {
        match definition {
            TypeSystemDefinition::Type(positioned) => {
                let type_def = positioned.node;
                let name = type_def.name.node.to_string();
                match type_def.kind {
                    TypeKind::Object(object) => {
                        let mut fields = BTreeMap::new();
                        for field in object.fields {
                            fields.insert(
                                field.node.name.node.to_string(),
                                render_type(&field.node.ty.node),
                            );
                        }
                        contract.objects.insert(name, fields);
                    }
                    TypeKind::Enum(enumeration) => {
                        let values = enumeration
                            .values
                            .into_iter()
                            .map(|value| value.node.value.node.to_string())
                            .collect();
                        contract.enums.insert(name, values);
                    }
                    TypeKind::Scalar => {
                        contract.scalars.insert(name);
                    }
                    _ => {}
                }
            }
            TypeSystemDefinition::Directive(positioned) => {
                contract
                    .directives
                    .insert(positioned.node.name.node.to_string());
            }
            TypeSystemDefinition::Schema(_) => {}
        }
    }

    contract
}

/// Render a parsed [`Type`] to its canonical SDL string, e.g. `String!`,
/// `[Role!]`, `DateTimeISO!`.
fn render_type(ty: &Type) -> String {
    let base = match &ty.base {
        BaseType::Named(name) => name.to_string(),
        BaseType::List(inner) => format!("[{}]", render_type(inner)),
    };
    if ty.nullable {
        base
    } else {
        format!("{base}!")
    }
}

fn compare(node: &Contract, rust: &Contract) -> Report {
    let mut breaking = Vec::new();
    let mut info = Vec::new();

    for (type_name, node_fields) in &node.objects {
        let Some(rust_fields) = rust.objects.get(type_name) else {
            breaking.push(format!(
                "type `{type_name}` is in the Node contract but missing from the Rust schema"
            ));
            continue;
        };
        for (field, node_type) in node_fields {
            match rust_fields.get(field) {
                None => breaking.push(format!(
                    "`{type_name}.{field}: {node_type}` is missing from the Rust schema"
                )),
                Some(rust_type) if rust_type != node_type => breaking.push(format!(
                    "`{type_name}.{field}` type differs: Node `{node_type}` vs Rust `{rust_type}`"
                )),
                Some(_) => {}
            }
        }
        for field in rust_fields.keys() {
            if !node_fields.contains_key(field) {
                info.push(format!(
                    "`{type_name}.{field}` added in Rust (not in Node) — additive"
                ));
            }
        }
    }
    for type_name in rust.objects.keys() {
        if !node.objects.contains_key(type_name) {
            info.push(format!(
                "type `{type_name}` added in Rust (not in Node) — additive"
            ));
        }
    }

    for (enum_name, node_values) in &node.enums {
        let Some(rust_values) = rust.enums.get(enum_name) else {
            breaking.push(format!(
                "enum `{enum_name}` is in the Node contract but missing from the Rust schema"
            ));
            continue;
        };
        for value in node_values {
            if !rust_values.contains(value) {
                breaking.push(format!(
                    "enum value `{enum_name}.{value}` is missing from the Rust schema"
                ));
            }
        }
        for value in rust_values {
            if !node_values.contains(value) {
                info.push(format!(
                    "enum value `{enum_name}.{value}` added in Rust — additive"
                ));
            }
        }
    }

    for scalar in &rust.scalars {
        if !node.scalars.contains(scalar) {
            info.push(format!(
                "scalar `{scalar}` declared in Rust (implicit in Node via graphql-scalars)"
            ));
        }
    }

    for directive in &node.directives {
        if !rust.directives.contains(directive) && !BUILTIN_DIRECTIVES.contains(&directive.as_str())
        {
            info.push(format!(
				"directive `@{directive}` in Node not in Rust schema (schema-side directive; does not affect SPA queries)"
			));
        }
    }

    Report {
        breaking,
        info,
        stale: None,
    }
}

fn staleness(out: &Path, emitted: &str) -> Option<String> {
    match std::fs::read_to_string(out) {
        Ok(committed) if committed == emitted => None,
        Ok(_) => Some(format!(
            "{} is out of date; run `emit-schema`",
            out.display()
        )),
        Err(_) => Some(format!(
            "{} does not exist; run `emit-schema`",
            out.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Contract, compare, normalize};
    use async_graphql::parser::parse_schema;

    fn contract(sdl: &str) -> Contract {
        normalize(parse_schema(sdl).expect("valid test SDL"))
    }

    #[test]
    fn identical_schemas_have_no_breaking_changes() {
        let node = contract("type User { id: String! }");
        let rust = contract("type User { id: String! }");
        assert!(compare(&node, &rust).breaking.is_empty());
    }

    #[test]
    fn removed_field_is_breaking() {
        let node = contract("type User { id: String! email: String! }");
        let rust = contract("type User { id: String! }");
        assert!(!compare(&node, &rust).breaking.is_empty());
    }

    #[test]
    fn changed_field_type_is_breaking() {
        let node = contract("type User { createdAt: DateTimeISO! }");
        let rust = contract("type User { createdAt: String! }");
        assert!(!compare(&node, &rust).breaking.is_empty());
    }

    #[test]
    fn added_field_is_additive_not_breaking() {
        let node = contract("type User { id: String! }");
        let rust = contract("type User { id: String! biography: String }");
        let report = compare(&node, &rust);
        assert!(report.breaking.is_empty());
        assert!(!report.info.is_empty());
    }

    #[test]
    fn missing_enum_value_is_breaking() {
        let node = contract("enum Role { ADMIN USER }");
        let rust = contract("enum Role { ADMIN }");
        assert!(!compare(&node, &rust).breaking.is_empty());
    }
}
