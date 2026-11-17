//! Mandatory relations whose key and same-as columns form foreign keys wider
//! than two columns, each backed by one unique index on the satellite table
//! and declared with `#[table_model(foreign_key(...))]`.

mod shared;

use diesel_builders::prelude::*;

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
/// A slot of a template.
pub struct Slot {
    /// Slot identity.
    id: i32,
    /// Owning template.
    template_id: i32,
    /// Asset model the slot accepts.
    model_id: i32,
    /// Role the slot plays.
    role_id: i32,
    /// Slot label.
    label: String,
}

unique_index!(slots::id, slots::template_id, slots::model_id);
unique_index!(slots::id, slots::template_id, slots::model_id, slots::role_id);
unique_index!(slots::id, slots::label);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = disposals)]
#[table_model(ancestors(templates))]
#[table_model(foreign_key(
    (slot_id, id, model_id, role_id),
    (slots::id, slots::template_id, slots::model_id, slots::role_id)
))]
/// A template disposing of the asset held by one of its slots, tied to the
/// slot by one four-column foreign key.
pub struct Disposal {
    /// Template identity.
    #[same_as(slots::template_id, slot_id)]
    id: i32,
    /// Slot the disposal is tied to.
    #[mandatory(slots)]
    slot_id: i32,
    /// Asset model of the slot.
    #[same_as(slots::model_id, slot_id)]
    model_id: i32,
    /// Role of the slot.
    #[same_as(slots::role_id, slot_id)]
    role_id: i32,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = transfers)]
#[table_model(ancestors(templates))]
#[table_model(foreign_key(
    (slot_id, id, model_id),
    (slots::id, slots::template_id, slots::model_id)
))]
#[table_model(foreign_key(
    (slot_id, id, model_id, role_id),
    (slots::id, slots::template_id, slots::model_id, slots::role_id)
))]
/// A template transferring the asset of one of its slots, tied to the slot
/// by a three-column and a four-column foreign key sharing `slot_id`, and by
/// the two-column pair `(slot_id, label)`.
pub struct Transfer {
    /// Template identity.
    #[same_as(slots::template_id, slot_id)]
    id: i32,
    /// Slot the transfer is tied to.
    #[mandatory(slots)]
    slot_id: i32,
    /// Asset model of the slot.
    #[same_as(slots::model_id, slot_id)]
    model_id: i32,
    /// Role of the slot.
    #[same_as(slots::role_id, slot_id)]
    role_id: i32,
    /// Label of the slot.
    #[same_as(slots::label, slot_id)]
    label: String,
}

diesel::allow_tables_to_appear_in_same_query!(templates, slots, disposals, transfers);

/// Opens a connection holding the tables of this test.
///
/// # Errors
/// Fails when the connection or a `CREATE TABLE` statement fails.
fn connection() -> Result<SqliteConnection, Box<dyn std::error::Error>> {
    let mut conn = shared::establish_connection()?;
    // DDL has no typed Diesel DSL.
    for statement in [
        "CREATE TABLE templates (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "CREATE TABLE slots (
            id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL REFERENCES templates(id),
            model_id INTEGER NOT NULL,
            role_id INTEGER NOT NULL,
            label TEXT NOT NULL,
            UNIQUE (id, template_id, model_id),
            UNIQUE (id, template_id, model_id, role_id),
            UNIQUE (id, label)
        )",
        "CREATE TABLE disposals (
            id INTEGER PRIMARY KEY REFERENCES templates(id),
            slot_id INTEGER NOT NULL,
            model_id INTEGER NOT NULL,
            role_id INTEGER NOT NULL,
            FOREIGN KEY (slot_id, id, model_id, role_id)
                REFERENCES slots(id, template_id, model_id, role_id)
        )",
        "CREATE TABLE transfers (
            id INTEGER PRIMARY KEY REFERENCES templates(id),
            slot_id INTEGER NOT NULL,
            model_id INTEGER NOT NULL,
            role_id INTEGER NOT NULL,
            label TEXT NOT NULL,
            FOREIGN KEY (slot_id, id, model_id) REFERENCES slots(id, template_id, model_id),
            FOREIGN KEY (slot_id, id, model_id, role_id)
                REFERENCES slots(id, template_id, model_id, role_id),
            FOREIGN KEY (slot_id, label) REFERENCES slots(id, label)
        )",
    ] {
        diesel::sql_query(statement).execute(&mut conn)?;
    }
    Ok(conn)
}

#[test]
fn test_four_column_same_as_key_fills_host_from_slot() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = connection()?;

    let disposal = disposals::table::builder()
        .name("Disposal")
        .slot(slots::table::builder().model_id(7).role_id(3).label("Bin"))
        .insert(&mut conn)?;

    let slot: Slot = disposal.slot(&mut conn)?;
    assert_eq!(slot.template_id(), disposal.id());
    assert_eq!(disposal.model_id(), &7);
    assert_eq!(disposal.role_id(), &3);

    let keys: Vec<_> = disposal
        .iter_match_simple::<(slots::id, slots::template_id, slots::model_id, slots::role_id)>()
        .collect();
    assert_eq!(
        keys,
        [(
            Some(disposal.slot_id()),
            (Some(disposal.id()), (Some(disposal.model_id()), (Some(disposal.role_id()),)))
        )]
    );
    Ok(())
}

#[test]
fn test_wide_keys_sharing_a_relation_key_resolve_separately()
-> Result<(), Box<dyn std::error::Error>> {
    let mut conn = connection()?;

    let transfer = transfers::table::builder()
        .name("Transfer")
        .slot(slots::table::builder().model_id(5).role_id(2))
        .label("Dock")
        .insert(&mut conn)?;

    let slot: Slot = transfer.slot(&mut conn)?;
    assert_eq!(slot.template_id(), transfer.id());
    assert_eq!(transfer.model_id(), &5);
    assert_eq!(transfer.role_id(), &2);
    assert_eq!(slot.label(), "Dock");

    assert_eq!(
        transfer.iter_match_simple::<(slots::id, slots::template_id, slots::model_id)>().count(),
        1
    );
    assert_eq!(
        transfer
            .iter_match_simple::<(slots::id, slots::template_id, slots::model_id, slots::role_id)>()
            .count(),
        1
    );
    assert_eq!(
        transfer.iter_match_simple::<(slots::id, slots::label)>().collect::<Vec<_>>(),
        [(Some(transfer.slot_id()), (Some(transfer.label()),))]
    );
    Ok(())
}
