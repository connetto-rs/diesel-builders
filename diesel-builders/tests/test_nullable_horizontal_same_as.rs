//! Horizontal same-as columns whose foreign column is nullable while the host
//! column is `NOT NULL`. A non-null foreign value fills the host column and a
//! `NULL` leaves the host column for the caller to set.

use diesel_builders::{BuilderError, IncompleteBuilderError, prelude::*};

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = templates)]
#[table_model(surrogate_key)]
/// A procedure template.
pub struct Template {
    /// Template identity.
    id: i32,
    /// Template label.
    name: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = slots)]
#[table_model(surrogate_key, foreign_key(template_id, (templates::id)))]
/// A slot of a template, optionally placed into another slot.
pub struct Slot {
    /// Slot identity.
    id: i32,
    /// Owning template.
    template_id: i32,
    /// Slot holding the assets of this slot, when there is one.
    placed_into_slot_id: Option<i32>,
}

unique_index!(slots::id, slots::template_id);
unique_index!(slots::id, slots::placed_into_slot_id);
unique_index!(slots::id, slots::template_id, slots::placed_into_slot_id);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = storages)]
#[table_model(ancestors(templates))]
#[table_model(foreign_key(
    (stored_asset_slot_id, id, stored_into_slot_id),
    (slots::id, slots::template_id, slots::placed_into_slot_id)
))]
/// A template storing the assets of one of its slots into another of its
/// slots, both mandatory.
pub struct Storage {
    /// Template identity.
    #[same_as(slots::template_id, (stored_into_slot_id, stored_asset_slot_id))]
    id: i32,
    /// Slot receiving the assets.
    #[same_as(slots::placed_into_slot_id, stored_asset_slot_id)]
    #[mandatory(slots)]
    stored_into_slot_id: i32,
    /// Slot whose assets are stored.
    #[mandatory(slots)]
    stored_asset_slot_id: i32,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = retrievals)]
#[table_model(ancestors(templates), foreign_key(retrieved_from_slot_id, (slots::id)))]
/// A template retrieving the assets of a slot out of the slot holding them.
pub struct Retrieval {
    /// Template identity.
    id: i32,
    /// Slot the assets are retrieved from.
    #[same_as(slots::placed_into_slot_id, retrieved_asset_slot_id)]
    retrieved_from_slot_id: i32,
    /// Slot whose assets are retrieved.
    #[discretionary(slots)]
    retrieved_asset_slot_id: i32,
}

diesel::allow_tables_to_appear_in_same_query!(templates, slots, storages, retrievals);

/// Opens a connection holding the tables of this test.
fn connection() -> SqliteConnection {
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory SQLite opens");
    // DDL and pragmas have no typed Diesel DSL.
    for statement in [
        "PRAGMA foreign_keys = ON",
        "CREATE TABLE templates (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "CREATE TABLE slots (
            id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL REFERENCES templates(id),
            placed_into_slot_id INTEGER,
            UNIQUE (id, template_id),
            UNIQUE (id, placed_into_slot_id),
            UNIQUE (id, template_id, placed_into_slot_id),
            FOREIGN KEY (placed_into_slot_id, template_id) REFERENCES slots(id, template_id)
        )",
        "CREATE TABLE storages (
            id INTEGER PRIMARY KEY REFERENCES templates(id),
            stored_into_slot_id INTEGER NOT NULL,
            stored_asset_slot_id INTEGER NOT NULL,
            FOREIGN KEY (stored_into_slot_id, id) REFERENCES slots(id, template_id),
            FOREIGN KEY (stored_asset_slot_id, id) REFERENCES slots(id, template_id),
            FOREIGN KEY (stored_asset_slot_id, id, stored_into_slot_id)
                REFERENCES slots(id, template_id, placed_into_slot_id)
        )",
        "CREATE TABLE retrievals (
            id INTEGER PRIMARY KEY REFERENCES templates(id),
            retrieved_from_slot_id INTEGER NOT NULL REFERENCES slots(id),
            retrieved_asset_slot_id INTEGER NOT NULL,
            FOREIGN KEY (retrieved_asset_slot_id, retrieved_from_slot_id)
                REFERENCES slots(id, placed_into_slot_id)
        )",
    ] {
        diesel::sql_query(statement).execute(&mut conn).expect("schema statement runs");
    }
    conn
}

/// Inserts a template with a holding slot and an asset slot placed into it,
/// and returns the two slots.
fn placed_slots(conn: &mut SqliteConnection) -> (Slot, Slot) {
    let template =
        templates::table::builder().name("Shelf").insert(conn).expect("template inserts");
    let holder = slots::table::builder()
        .template_id(*template.id())
        .placed_into_slot_id(None)
        .insert(conn)
        .expect("holding slot inserts");
    let asset = slots::table::builder()
        .template_id(*template.id())
        .placed_into_slot_id(Some(*holder.id()))
        .insert(conn)
        .expect("asset slot inserts");
    (holder, asset)
}

#[test]
fn test_placed_model_fills_not_null_host() {
    let mut conn = connection();
    let (holder, asset) = placed_slots(&mut conn);

    let retrieval = retrievals::table::builder()
        .name("Retrieval")
        .retrieved_asset_slot_model(&asset)
        .insert(&mut conn)
        .expect("retrieval inserts");

    assert_eq!(retrieval.retrieved_from_slot_id(), holder.id());
    assert_eq!(retrieval.retrieved_asset_slot_id(), asset.id());
}

#[test]
fn test_placed_builder_fills_not_null_host() {
    let mut conn = connection();
    let (holder, _) = placed_slots(&mut conn);

    let retrieval = retrievals::table::builder()
        .name("Retrieval")
        .retrieved_asset_slot(
            slots::table::builder()
                .template_id(*holder.template_id())
                .placed_into_slot_id(Some(*holder.id())),
        )
        .insert(&mut conn)
        .expect("retrieval inserts");

    assert_eq!(retrieval.retrieved_from_slot_id(), holder.id());
    let asset: Slot = retrieval.retrieved_asset_slot(&mut conn).expect("asset slot loads");
    assert_eq!(asset.placed_into_slot_id(), &Some(*holder.id()));
}

#[test]
fn test_unplaced_model_leaves_not_null_host_unset() {
    let mut conn = connection();
    let (holder, _) = placed_slots(&mut conn);

    let result = retrievals::table::builder()
        .name("Retrieval")
        .retrieved_asset_slot_model(&holder)
        .insert(&mut conn);

    assert!(matches!(
        result,
        Err(BuilderError::Incomplete(IncompleteBuilderError::MissingMandatoryField {
            table_name: "retrievals",
            field_name: "retrieved_from_slot_id",
        }))
    ));
}

#[test]
fn test_unplaced_model_keeps_caller_host_value() {
    let mut conn = connection();
    let (holder, asset) = placed_slots(&mut conn);

    let builder = retrievals::table::builder()
        .name("Retrieval")
        .retrieved_from_slot_id(*asset.id())
        .retrieved_asset_slot_model(&holder);

    assert_eq!(builder.may_get_column::<retrievals::retrieved_from_slot_id>(), Some(*asset.id()));
    let result = builder.insert(&mut conn);
    assert!(matches!(result, Err(BuilderError::Diesel(_))));
}

#[test]
fn test_unplaced_builder_leaves_not_null_host_unset() {
    let mut conn = connection();
    let (holder, _) = placed_slots(&mut conn);

    let builder = retrievals::table::builder().retrieved_asset_slot(
        slots::table::builder().template_id(*holder.template_id()).placed_into_slot_id(None),
    );

    assert_eq!(builder.may_get_column::<retrievals::retrieved_from_slot_id>(), None);
}

#[test]
fn test_mandatory_storage_inherits_placement_from_asset_slot_builder() {
    let mut conn = connection();
    let (holder, _) = placed_slots(&mut conn);

    let builder = storages::table::builder()
        .stored_asset_slot(slots::table::builder().placed_into_slot_id(Some(*holder.id())));

    assert_eq!(builder.may_get_column::<storages::stored_into_slot_id>(), Some(*holder.id()));
}
