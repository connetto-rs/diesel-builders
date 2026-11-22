//! A `NULL` set on a nullable same-as source reaches its nullable targets in
//! ancestor tables and in mandatory and discretionary side builders, and
//! leaves `NOT NULL` targets unset.

use diesel_builders::prelude::*;

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = parents)]
#[table_model(surrogate_key)]
/// An ancestor table holding the targets of vertical same-as columns.
pub struct Parent {
    /// Parent identity.
    id: i32,
    /// Nullable target of `children::child_note`.
    note: Option<String>,
    /// `NOT NULL` target of `children::child_label`.
    label: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = children)]
#[table_model(ancestors(parents))]
/// A descendant table whose nullable columns feed its ancestor.
pub struct Child {
    /// Child identity.
    id: i32,
    /// Nullable source of `parents::note`.
    #[same_as(parents::note)]
    remark: Option<String>,
    /// Nullable source of `parents::label`.
    #[same_as(parents::label)]
    caption: Option<String>,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = sides)]
#[table_model(surrogate_key, foreign_key(parent_id, (parents::id)))]
/// A side table referenced by `hosts` through mandatory and discretionary
/// keys.
pub struct Side {
    /// Side identity.
    id: i32,
    /// Parent the side belongs to.
    parent_id: i32,
    /// Nullable target of `hosts::tag`.
    tag: Option<String>,
}

unique_index!(sides::id, sides::tag);
unique_index!(sides::id, sides::parent_id);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = hosts)]
#[table_model(ancestors(parents))]
/// A descendant table whose nullable `tag` feeds both of its side rows.
pub struct Host {
    /// Host identity.
    #[same_as(sides::parent_id, (mandatory_id, discretionary_id))]
    id: i32,
    /// Mandatory side row.
    #[mandatory(sides)]
    mandatory_id: i32,
    /// Discretionary side row.
    #[discretionary(sides)]
    discretionary_id: i32,
    /// Nullable source of the `tag` of both side rows.
    #[same_as(sides::tag, (mandatory_id, discretionary_id))]
    tag: Option<String>,
}

diesel::allow_tables_to_appear_in_same_query!(parents, children, hosts);

/// Opens a connection holding the tables of this test.
fn connection() -> SqliteConnection {
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory SQLite opens");
    // DDL and pragmas have no typed Diesel DSL.
    for statement in [
        "PRAGMA foreign_keys = ON",
        "CREATE TABLE parents (id INTEGER PRIMARY KEY, note TEXT, label TEXT NOT NULL)",
        "CREATE TABLE children (
            id INTEGER PRIMARY KEY REFERENCES parents(id),
            remark TEXT,
            caption TEXT
        )",
        "CREATE TABLE sides (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NOT NULL REFERENCES parents(id),
            tag TEXT,
            UNIQUE (id, tag),
            UNIQUE (id, parent_id)
        )",
        "CREATE TABLE hosts (
            id INTEGER PRIMARY KEY REFERENCES parents(id),
            mandatory_id INTEGER NOT NULL REFERENCES sides(id),
            discretionary_id INTEGER NOT NULL REFERENCES sides(id),
            tag TEXT,
            FOREIGN KEY (mandatory_id, tag) REFERENCES sides(id, tag),
            FOREIGN KEY (discretionary_id, tag) REFERENCES sides(id, tag),
            FOREIGN KEY (mandatory_id, id) REFERENCES sides(id, parent_id),
            FOREIGN KEY (discretionary_id, id) REFERENCES sides(id, parent_id)
        )",
    ] {
        diesel::sql_query(statement).execute(&mut conn).expect("schema statement runs");
    }
    conn
}

#[test]
fn null_child_column_overwrites_nullable_ancestor_column() {
    let mut conn = connection();
    let child = children::table::builder()
        .label("Label")
        .remark(Some("Stale".to_owned()))
        .remark(None)
        .insert(&mut conn)
        .expect("child inserts");
    let parent: Parent = child.ancestor(&mut conn).expect("parent loads");
    assert_eq!(parent.note(), &None);
}

#[test]
fn fallible_null_child_column_overwrites_nullable_ancestor_column() {
    let mut conn = connection();
    let mut builder = children::table::builder().label("Label");
    builder
        .try_set_column_ref::<children::remark>(Some("Stale".to_owned()))
        .expect("infallible")
        .try_set_column_ref::<children::remark>(None)
        .expect("infallible");
    let child = builder.insert(&mut conn).expect("child inserts");
    let parent: Parent = child.ancestor(&mut conn).expect("parent loads");
    assert_eq!(parent.note(), &None);
}

#[test]
fn null_child_column_leaves_not_null_ancestor_column_unset() {
    let builder = children::table::builder().caption(None);
    assert_eq!(builder.may_get_column::<parents::label>(), None);
}

#[test]
fn null_host_column_overwrites_values_in_side_builders() {
    let mut conn = connection();
    let host = hosts::table::builder()
        .label("Label")
        .mandatory(sides::table::builder().tag(Some("Mandatory".to_owned())))
        .discretionary(sides::table::builder().tag(Some("Discretionary".to_owned())))
        .tag(None)
        .insert(&mut conn)
        .expect("host inserts");
    assert_eq!(host.tag(), &None);
    assert_eq!(host.mandatory(&mut conn).expect("mandatory side loads").tag(), &None);
    assert_eq!(host.discretionary(&mut conn).expect("discretionary side loads").tag(), &None);
}
