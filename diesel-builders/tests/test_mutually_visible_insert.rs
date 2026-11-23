//! Tests for a nested insert into tables whose rows become visible only once
//! the rows of the other tables exist, as under row-level security policies
//! that each require the other table's row.

mod shared;
use diesel::{connection::SimpleConnection, prelude::*};
use diesel_builders::prelude::*;

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = resources)]
/// A resource, visible only once its ownable exists.
pub struct Resource {
    /// Primary key.
    id: i32,
    /// The kind of resource.
    kind: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[table_model(ancestors(resources))]
#[diesel(table_name = ownables)]
/// An ownable extending a resource, visible only once its resource is.
pub struct Ownable {
    /// Primary key, shared with the resource.
    id: i32,
    /// The project owning the resource.
    project: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[table_model(ancestors(resources, ownables))]
#[diesel(table_name = documents)]
/// A document extending an ownable.
pub struct Document {
    /// Primary key, shared with the ownable.
    id: i32,
    /// The document title.
    title: String,
}

diesel::allow_tables_to_appear_in_same_query!(resources, ownables, documents);

/// Opens a connection whose `resources` and `ownables` views each show a row
/// only once the row of the other exists.
fn connection_with_views() -> SqliteConnection {
    let mut connection = shared::establish_connection().expect("connection opens");
    // Diesel has no DSL for DDL.
    connection
        .batch_execute(
            "CREATE TABLE resource_rows (id INTEGER PRIMARY KEY NOT NULL, kind TEXT NOT NULL);
            CREATE TABLE ownable_rows (
                id INTEGER PRIMARY KEY NOT NULL REFERENCES resource_rows(id),
                project TEXT NOT NULL
            );
            CREATE TABLE documents (
                id INTEGER PRIMARY KEY NOT NULL REFERENCES ownable_rows(id),
                title TEXT NOT NULL
            );
            CREATE VIEW resources AS SELECT id, kind FROM resource_rows
                WHERE id IN (SELECT id FROM ownable_rows);
            CREATE VIEW ownables AS SELECT id, project FROM ownable_rows
                WHERE id IN (SELECT id FROM resources);
            CREATE TRIGGER resources_insert INSTEAD OF INSERT ON resources BEGIN
                INSERT INTO resource_rows (id, kind) VALUES (NEW.id, NEW.kind);
            END;
            CREATE TRIGGER ownables_insert INSTEAD OF INSERT ON ownables BEGIN
                INSERT INTO ownable_rows (id, project) VALUES (NEW.id, NEW.project);
            END;",
        )
        .expect("views are created");
    connection
}

#[test]
fn insert_reads_rows_once_every_table_is_written() {
    let mut connection = connection_with_views();

    let ownable = SetResourceId::id(ownables::table::builder(), 1)
        .kind("ownable")
        .project("lab")
        .insert(&mut connection)
        .expect("ownable inserts");

    assert_eq!(ownable, Ownable { id: 1, project: "lab".to_owned() });
    let resource: Resource = ownable.ancestor(&mut connection).expect("resource loads");
    assert_eq!(resource, Resource { id: 1, kind: "ownable".to_owned() });
}

#[test]
fn nested_insert_reads_rows_once_every_table_is_written() {
    let mut connection = connection_with_views();

    let (resource, (ownable, (document,))) = SetResourceId::id(documents::table::builder(), 1)
        .kind("document")
        .project("lab")
        .title("protocol")
        .insert_nested(&mut connection)
        .expect("document inserts");

    assert_eq!(resource, Resource { id: 1, kind: "document".to_owned() });
    assert_eq!(ownable, Ownable { id: 1, project: "lab".to_owned() });
    assert_eq!(document, Document { id: 1, title: "protocol".to_owned() });
}
