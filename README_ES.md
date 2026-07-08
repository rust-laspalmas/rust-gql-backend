<a href="https://graphql.org/"><img src="imgs/graphql.svg" align="left" width="250" alt="GraphQL"/></a>

# rust-gql-backend

> Backend GraphQL en Rust — **async-graphql + axum + sqlx** — que emite el contrato
> único `schema.graphql`.

<br clear="left"/>

English: [README.md](./README.md)

Un port a Rust del backend del taller GraphQL de GofiGeeks. Sirve queries, una
mutation de upload multipart, subscriptions (SSE **y** WebSocket) y auth de sesión
Better-Auth — todo config/feature-driven. **Emite** `schema.graphql`, el contrato
que comparten todos los clientes.

## Workspace

- **`crates/api`** — lib (schema, datos, transporte, auth, storage) + bin `gql-api`.
- **`xtask`** — `emit-schema` + `diff` (la puerta de contrato).
- **`xcheck`** — contrato cruzado (un request equivalente al de Leptos vía graphql-client).

## Requisitos

- Rust **1.95+**
- **Postgres** con el schema de Better-Auth (sólo para lo que toca DB; `{ hello }`
  funciona sin ella)
- Secretos por variables de entorno con prefijo `GQL_` (`config.toml` guarda los
  defaults no-secretos)

## Ejecutar

```bash
# config: config.toml + entorno GQL_ (GQL_DATABASE__URL, GQL_AUTH__SESSION__SECRET, ...)
cargo run -p api                 # http://localhost:4000/graphql (+ GraphiQL en GET)

# emitir + verificar el contrato
cargo run -p xtask -- emit-schema     # -> schema.graphql
cargo run -p xtask -- diff            # falla ante cambios breaking / desactualizado

cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## El ecosistema

| Repo | Qué aporta |
|------|------------|
| [rust-gql-domain](https://github.com/rust-lasplamas/rust-gql-domain) | newtypes + validación compartidos (este crate depende de él) |
| **rust-gql-backend** (este) | el servidor; **emite** `schema.graphql` |
| [rust-gql-frontend](https://github.com/rust-lasplamas/rust-gql-frontend) | cliente Leptos que compila contra el schema emitido |
| [rust-gql-docs](https://github.com/rust-lasplamas/rust-gql-docs) | mdBook bilingüe + rustdoc |

`schema.graphql` es el contrato único: un cambio incompatible rompe la compilación
de todos los consumidores Rust, y `xtask diff` lo caza en CI.

---

<a href="https://rust-laspalmas.dev/"><img src="imgs/rust-laspalmas.svg" align="left" width="150" alt="Rust Las Palmas"/></a>

<br>

    
Parte de la exploración de aprendizaje **GraphQL de GofiGeeks → Rust** por
[Rust Las Palmas](https://rust-lasplamas.dev) · [jesusperez.pro](https://jesusperez.pro).

No es un fork del taller — es un complemento.

<br clear="left"/>
