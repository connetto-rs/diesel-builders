//! Tests for columns the database generates, which builders and upserts must
//! never write.

mod shared;
use diesel::{connection::SimpleConnection, prelude::*};
use diesel_builders::prelude::*;

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = probes)]
/// Model for the `probes` table.
pub struct Probe {
    /// Primary key.
    id: i32,
    /// A nullable value the generated columns derive from.
    value: Option<String>,
    /// A nullable generated column under another Rust name, which also checks
    /// that `generated` is found after a `key = value` entry.
    #[table_model(sql_name = "size", generated)]
    value_length: Option<i32>,
    /// A non-nullable generated column.
    #[table_model(generated)]
    kind: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = grantees)]
/// Supertype table whose `kind` discriminates the subtype.
pub struct Grantee {
    /// Primary key.
    id: i32,
    /// Discriminator naming the subtype.
    kind: String,
}

unique_index!(grantees::id, grantees::kind);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = members)]
#[table_model(foreign_key((id, kind), (grantees::id, grantees::kind)))]
/// Subtype table whose generated `kind` hosts a composite foreign key.
pub struct Member {
    /// Primary key.
    id: i32,
    /// Discriminator the database fills in.
    #[table_model(generated)]
    kind: String,
    /// A plain column.
    name: String,
}

diesel::allow_tables_to_appear_in_same_query!(grantees, members);

/// Opens a connection holding the `probes`, `grantees` and `members` tables.
fn connection_with_tables() -> SqliteConnection {
    let mut connection = shared::establish_connection().expect("connection opens");
    // Diesel has no DSL for DDL.
    connection
        .batch_execute(
            "CREATE TABLE probes (
                id INTEGER PRIMARY KEY NOT NULL,
                value TEXT,
                size INTEGER GENERATED ALWAYS AS (length(value)) STORED,
                kind TEXT NOT NULL GENERATED ALWAYS AS ('probe') STORED
            );
            CREATE TABLE grantees (
                id INTEGER PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                UNIQUE (id, kind)
            );
            CREATE TABLE members (
                id INTEGER PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL GENERATED ALWAYS AS ('member') STORED,
                name TEXT NOT NULL,
                FOREIGN KEY (id, kind) REFERENCES grantees (id, kind)
            );",
        )
        .expect("tables are created");
    connection
}

#[test]
fn builder_insert_returns_generated_values() {
    let mut connection = connection_with_tables();

    let probe = probes::table::builder()
        .id(1)
        .value(Some("abc".to_owned()))
        .insert(&mut connection)
        .expect("builder inserts without writing generated columns");
    assert_eq!(probe.value_length(), &Some(3));
    assert_eq!(probe.kind(), "probe");

    let probe = probes::table::builder()
        .id(2)
        .insert(&mut connection)
        .expect("generated columns are not mandatory");
    assert_eq!(probe.value_length(), &None);
    assert_eq!(probe.kind(), "probe");

    let loaded: Probe = Probe::find(&2, &mut connection).expect("probe loads");
    assert_eq!(loaded, probe);
}

#[test]
fn upsert_recomputes_generated_values() {
    let mut connection = connection_with_tables();
    let mut probe = probes::table::builder()
        .id(1)
        .value(Some("abc".to_owned()))
        .insert(&mut connection)
        .expect("probe inserts");

    probe.value = Some("abcdef".to_owned());
    let updated = probe.upsert(&mut connection).expect("upsert skips generated columns");
    assert_eq!(updated.value(), &Some("abcdef".to_owned()));
    assert_eq!(updated.value_length(), &Some(6));
    assert_eq!(updated.kind(), "probe");

    let inserted = Probe { id: 2, value: None, value_length: Some(99), kind: "ignored".to_owned() }
        .upsert(&mut connection)
        .expect("upsert inserts without generated columns");
    assert_eq!(inserted.value_length(), &None);
    assert_eq!(inserted.kind(), "probe");
}

#[test]
fn generated_column_hosts_foreign_key() {
    let mut connection = connection_with_tables();
    let grantee = grantees::table::builder()
        .id(1)
        .kind("member")
        .insert(&mut connection)
        .expect("grantee inserts");
    let member = members::table::builder()
        .id(1)
        .name("Ada")
        .insert(&mut connection)
        .expect("member inserts and satisfies the composite foreign key");
    assert_eq!(member.kind(), "member");

    let referenced: Grantee = member
        .foreign::<(members::id, members::kind), (grantees::id, grantees::kind)>(&mut connection)
        .expect("generated host column resolves the foreign row");
    assert_eq!(referenced, grantee);
}
