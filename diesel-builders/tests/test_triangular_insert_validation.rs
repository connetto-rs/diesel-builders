//! Tests for validation errors raised while a triangular child inserts its
//! associated records.

mod shared;
mod shared_triangular;
use std::convert::Infallible;

use diesel::{connection::SimpleConnection, prelude::*};
use diesel_builders::prelude::*;
use shared_triangular::*;

diesel::allow_tables_to_appear_in_same_query!(mandatory_children, parent_table);
diesel::allow_tables_to_appear_in_same_query!(mandatory_children, satellite_table);
diesel::allow_tables_to_appear_in_same_query!(discretionary_children, parent_table);
diesel::allow_tables_to_appear_in_same_query!(discretionary_children, satellite_table);
diesel::allow_tables_to_appear_in_same_query!(checked_satellites, parent_table);
diesel::allow_tables_to_appear_in_same_query!(checked_children, parent_table);
diesel::allow_tables_to_appear_in_same_query!(checked_children, checked_satellites);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[table_model(error = RejectedKey, ancestors = parent_table)]
#[diesel(table_name = mandatory_children)]
/// Child whose mandatory satellite key is validated.
pub struct MandatoryChild {
    #[infallible]
    #[same_as(satellite_table::parent_id)]
    /// Primary key.
    id: i32,
    #[mandatory(satellite_table)]
    /// Key of the mandatory satellite.
    satellite_id: i32,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[table_model(error = RejectedKey, ancestors = parent_table)]
#[diesel(table_name = discretionary_children)]
/// Child whose discretionary satellite key is validated.
pub struct DiscretionaryChild {
    #[infallible]
    #[same_as(satellite_table::parent_id)]
    /// Primary key.
    id: i32,
    #[discretionary(satellite_table)]
    /// Key of the discretionary satellite.
    satellite_id: i32,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[table_model(surrogate_key, record_error = RejectedRecord)]
#[diesel(table_name = checked_satellites)]
#[table_model(foreign_key(parent_id, (parent_table::id)))]
/// Satellite whose record validator rejects some values.
pub struct CheckedSatellite {
    /// Primary key.
    id: i32,
    /// Foreign key to the parent table.
    parent_id: i32,
    /// A value the record validator checks.
    field: String,
}

unique_index!(checked_satellites::id, checked_satellites::parent_id);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[table_model(ancestors = parent_table)]
#[diesel(table_name = checked_children)]
/// Child whose discretionary satellite validates its own record.
pub struct CheckedChild {
    #[same_as(checked_satellites::parent_id)]
    /// Primary key.
    id: i32,
    #[discretionary(checked_satellites)]
    /// Key of the discretionary satellite.
    checked_satellite_id: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("satellite key {0} is rejected")]
/// A satellite key the child refuses to reference.
pub struct RejectedKey(i32);

impl From<Infallible> for RejectedKey {
    fn from(inf: Infallible) -> Self {
        match inf {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("satellite record is rejected")]
/// A satellite record its validator refuses.
pub struct RejectedRecord;

impl ValidateColumn<mandatory_children::satellite_id>
    for <mandatory_children::table as TableExt>::NewValues
{
    type Error = RejectedKey;

    fn validate_column(value: &i32) -> Result<(), Self::Error> {
        Err(RejectedKey(*value))
    }
}

impl ValidateColumn<discretionary_children::satellite_id>
    for <discretionary_children::table as TableExt>::NewValues
{
    type Error = RejectedKey;

    fn validate_column(value: &i32) -> Result<(), Self::Error> {
        Err(RejectedKey(*value))
    }
}

impl ValidateRecord<checked_satellites::table>
    for <checked_satellites::table as TableExt>::NewValues
{
    fn validate_record(&self) -> Result<(), RejectedRecord> {
        if self
            .may_get_column_ref::<checked_satellites::field>()
            .is_some_and(|field| field == "rejected")
        {
            Err(RejectedRecord)
        } else {
            Ok(())
        }
    }
}

/// Opens a connection holding the satellite tables and the child tables.
fn connection_with_children() -> SqliteConnection {
    let mut connection = shared::establish_connection().expect("connection opens");
    setup_triangular_tables(&mut connection).expect("triangular tables are created");
    // Diesel has no DSL for DDL.
    connection
        .batch_execute(
            "CREATE TABLE mandatory_children (
                id INTEGER PRIMARY KEY NOT NULL REFERENCES parent_table(id),
                satellite_id INTEGER NOT NULL REFERENCES satellite_table(id)
            );
            CREATE TABLE discretionary_children (
                id INTEGER PRIMARY KEY NOT NULL REFERENCES parent_table(id),
                satellite_id INTEGER NOT NULL REFERENCES satellite_table(id)
            );
            CREATE TABLE checked_satellites (
                id INTEGER PRIMARY KEY NOT NULL,
                parent_id INTEGER NOT NULL REFERENCES parent_table(id),
                field TEXT NOT NULL,
                UNIQUE(id, parent_id)
            );
            CREATE TABLE checked_children (
                id INTEGER PRIMARY KEY NOT NULL REFERENCES parent_table(id),
                checked_satellite_id INTEGER NOT NULL REFERENCES checked_satellites(id)
            );",
        )
        .expect("child tables are created");
    connection
}

#[test]
fn mandatory_key_rejected_after_satellite_insert() {
    let mut connection = connection_with_children();
    let satellite = satellite_table::table::builder().field("satellite");

    let error = mandatory_children::table::builder()
        .parent_field("parent")
        .try_satellite(satellite)
        .expect("the satellite builder is accepted")
        .insert(&mut connection)
        .expect_err("the inserted satellite key is rejected");

    assert_eq!(error.to_string(), "Validation error: satellite key 1 is rejected");
    let children: i64 =
        mandatory_children::table.count().get_result(&mut connection).expect("children count");
    assert_eq!(children, 0);
}

#[test]
fn discretionary_key_rejected_after_satellite_insert() {
    let mut connection = connection_with_children();
    let satellite = satellite_table::table::builder().field("satellite");

    let error = discretionary_children::table::builder()
        .parent_field("parent")
        .try_satellite(satellite)
        .expect("the satellite builder is accepted")
        .insert(&mut connection)
        .expect_err("the inserted satellite key is rejected");

    assert_eq!(error.to_string(), "Validation error: satellite key 1 is rejected");
    let children: i64 =
        discretionary_children::table.count().get_result(&mut connection).expect("children count");
    assert_eq!(children, 0);
}

#[test]
fn discretionary_record_rejected_on_satellite_insert() {
    let mut connection = connection_with_children();
    let satellite = checked_satellites::table::builder().field("rejected");

    let error = checked_children::table::builder()
        .parent_field("parent")
        .checked_satellite(satellite)
        .insert(&mut connection)
        .expect_err("the satellite record is rejected");

    assert_eq!(error.to_string(), "Validation error: satellite record is rejected");
    let satellites: i64 =
        checked_satellites::table.count().get_result(&mut connection).expect("satellites count");
    assert_eq!(satellites, 0);
}
