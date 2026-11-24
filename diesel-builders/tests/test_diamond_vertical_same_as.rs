//! Test case for a diamond whose descendant column is vertically same-as the
//! matching column of both of its parents.
//!
//! `pets` extends `dogs` and `cats`, which both extend `animals`. Every table
//! carries an `owner` column, and each descendant declares its `owner` equal to
//! the `owner` of every parent.

mod shared;

use std::convert::Infallible;

use diesel::prelude::*;
use diesel_builders::{TrySetHomogeneousNestedColumns, prelude::*};

diesel::allow_tables_to_appear_in_same_query!(animals, dogs, cats, pets);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = animals)]
#[table_model(surrogate_key)]
/// Root of the diamond.
pub struct Animal {
    /// Primary key.
    id: i32,
    /// Owner of the animal.
    owner: String,
}

unique_index!(animals::id, animals::owner);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = dogs)]
#[table_model(ancestors(animals))]
/// Left side of the diamond.
pub struct Dog {
    /// Primary key.
    id: i32,
    /// Owner of the dog.
    #[same_as(animals::owner)]
    owner: String,
}

unique_index!(dogs::id, dogs::owner);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = cats)]
#[table_model(ancestors(animals))]
/// Right side of the diamond.
pub struct Cat {
    /// Primary key.
    id: i32,
    /// Owner of the cat.
    #[same_as(animals::owner)]
    owner: String,
}

unique_index!(cats::id, cats::owner);

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = pets)]
#[table_model(ancestors(animals, dogs, cats))]
/// Bottom of the diamond, extending both `dogs` and `cats`.
pub struct Pet {
    /// Primary key.
    id: i32,
    /// Owner of the pet.
    #[same_as(dogs::owner)]
    #[same_as(cats::owner)]
    owner: String,
}

#[test]
fn test_diamond_vertical_same_as() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = shared::establish_connection()?;

    // Diesel has no DSL for DDL, so the schema is created from raw statements.
    for statement in [
        "CREATE TABLE animals (
            id INTEGER PRIMARY KEY NOT NULL,
            owner TEXT NOT NULL,
            UNIQUE (id, owner)
        )",
        "CREATE TABLE dogs (
            id INTEGER PRIMARY KEY NOT NULL REFERENCES animals(id),
            owner TEXT NOT NULL,
            UNIQUE (id, owner),
            FOREIGN KEY (id, owner) REFERENCES animals(id, owner)
        )",
        "CREATE TABLE cats (
            id INTEGER PRIMARY KEY NOT NULL REFERENCES animals(id),
            owner TEXT NOT NULL,
            UNIQUE (id, owner),
            FOREIGN KEY (id, owner) REFERENCES animals(id, owner)
        )",
        "CREATE TABLE pets (
            id INTEGER PRIMARY KEY NOT NULL,
            owner TEXT NOT NULL,
            FOREIGN KEY (id) REFERENCES dogs(id),
            FOREIGN KEY (id) REFERENCES cats(id),
            FOREIGN KEY (id, owner) REFERENCES dogs(id, owner),
            FOREIGN KEY (id, owner) REFERENCES cats(id, owner)
        )",
    ] {
        diesel::sql_query(statement).execute(&mut conn)?;
    }

    let alice =
        pets::table::builder().set_column::<pets::owner>("Alice".to_owned()).insert(&mut conn)?;
    let bob = pets::table::builder()
        .try_set_column::<pets::owner>("Bob".to_owned())?
        .insert(&mut conn)?;

    for (pet, owner) in [(alice, "Alice"), (bob, "Bob")] {
        let animal: Animal =
            animals::table.find(pet.id()).select(Animal::as_select()).first(&mut conn)?;
        let dog: Dog = dogs::table.find(pet.id()).select(Dog::as_select()).first(&mut conn)?;
        let cat: Cat = cats::table.find(pet.id()).select(Cat::as_select()).first(&mut conn)?;
        assert_eq!(pet.owner(), owner);
        assert_eq!(animal.owner(), owner);
        assert_eq!(dog.owner(), owner);
        assert_eq!(cat.owner(), owner);
    }

    let mut builder = pets::table::builder();
    TrySetHomogeneousNestedColumns::<String, Infallible, (dogs::owner, (cats::owner,))>::try_set_homogeneous_nested_columns(
        &mut builder,
        &"Carol".to_owned(),
    )?;
    assert_eq!(builder.may_get_column_ref::<dogs::owner>().map(String::as_str), Some("Carol"));
    assert_eq!(builder.may_get_column_ref::<cats::owner>().map(String::as_str), Some("Carol"));

    Ok(())
}
