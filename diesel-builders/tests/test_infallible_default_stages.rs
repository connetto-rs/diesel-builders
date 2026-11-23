//! Struct default overrides on tables without any validation, whose default
//! stages all fail with `Infallible`. The derive output maps each stage error
//! into the composed `DefaultError`, and that mapping must not trip
//! `unreachable_code` on an uninhabited error.
#![deny(unreachable_code)]

use diesel_builders::prelude::*;

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = animals)]
/// Root model without validation.
pub struct Animal {
    /// Primary key.
    id: i32,
    /// Kind of the animal.
    kind: String,
    /// Name of the animal.
    name: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = dogs)]
#[table_model(ancestors(animals))]
#[table_model(default(animals::kind, "dog"))]
#[table_model(default(animals::name, "Rex"))]
/// Descendant overriding two ancestor defaults, so its default error composes
/// a base stage, a middle stage and a last stage.
pub struct Dog {
    /// Primary key.
    id: i32,
}

diesel::allow_tables_to_appear_in_same_query!(animals, dogs);

#[test]
fn test_infallible_default_stages_apply_overrides() {
    let builder = dogs::table::builder();

    assert_eq!(builder.may_get_column::<animals::kind>(), Some("dog".to_owned()));
    assert_eq!(builder.may_get_column::<animals::name>(), Some("Rex".to_owned()));
}
