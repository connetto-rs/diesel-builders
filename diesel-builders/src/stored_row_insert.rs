//! Writing the rows of an insert and reading back the rows the database
//! stored.
//!
//! A nested insert writes every row before it reads any. Row-level security
//! policies can make a row visible only once the rows of other tables exist,
//! so a row read right after its own insert may not be visible yet.

use diesel::{Insertable, RunQueryDsl, query_builder::InsertStatement, query_dsl::LoadQuery};
use tuplities::prelude::{
    FlattenNestedTuple, IntoNestedTupleOption, NestedTupleInto, NestedTupleOption,
    NestedTupleOptionWith,
};

use crate::{
    LoadQueryBuilder, MayGetNestedColumns, TableExt, TypedNestedTuple,
    builder_bundle::BundlableTableExt,
    columns::{NestedColumns, TupleEqAll},
};

/// The key source of a table whose primary key the builder holds before the
/// insert.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SuppliedKey;

/// The key source of a table whose primary key the database assigns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SurrogateKey;

/// The primary key values of a row of `T`.
pub(crate) type PrimaryKeyValues<T> =
    <<T as TableExt>::NestedPrimaryKeyColumns as TypedNestedTuple>::NestedTupleColumnType;

/// A table whose complete new values convert into a Diesel insertable.
pub(crate) trait InsertValues: BundlableTableExt {
    /// The insertable the new values convert into.
    type Values: Insertable<Self>;

    /// Converts new values holding every column of the new record.
    ///
    /// # Panics
    ///
    /// Panics when a column of the new record is missing, which callers rule
    /// out before converting.
    fn insert_values(new_values: Self::NewValues) -> Self::Values;
}

impl<T> InsertValues for T
where
    T: BundlableTableExt,
    T::NewRecord: TupleEqAll<EqAll: FlattenNestedTuple<Flattened: Insertable<T>>>
        + TypedNestedTuple<NestedTupleColumnType = T::CompletedNewValues>,
{
    type Values = <<T::NewRecord as TupleEqAll>::EqAll as FlattenNestedTuple>::Flattened;

    fn insert_values(new_values: Self::NewValues) -> Self::Values {
        #[expect(
            clippy::expect_used,
            reason = "callers check that every column of the new record is present before \
                      converting, so transpose_or cannot observe a missing column here"
        )]
        let values: T::CompletedNewValues = new_values
            .transpose_or(T::NewRecord::NESTED_COLUMN_NAMES)
            .expect("every required column was already proven present");
        T::NewRecord::default().eq_all(values).flatten()
    }
}

/// A key source writing a row of `T` without reading it back, returning the
/// row's primary key.
pub(crate) trait WriteRow<Conn, T: InsertValues> {
    /// Writes the row holding `new_values` and returns its primary key.
    ///
    /// # Errors
    ///
    /// Returns the database error of the insert.
    fn write_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<PrimaryKeyValues<T>>;
}

/// The key is taken from the new values, so the insert reports nothing back
/// and applies no `SELECT` policy to the new row.
impl<Conn, T> WriteRow<Conn, T> for SuppliedKey
where
    T: InsertValues,
    T::NewValues: MayGetNestedColumns<T::NestedPrimaryKeyColumns>,
    <PrimaryKeyValues<T> as IntoNestedTupleOption>::IntoOptions:
        NestedTupleOption<Transposed = PrimaryKeyValues<T>>,
    InsertStatement<T, <T::Values as Insertable<T>>::Values>:
        diesel::query_dsl::methods::ExecuteDsl<Conn>,
    Conn: diesel::Connection,
{
    fn write_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<PrimaryKeyValues<T>> {
        #[expect(
            clippy::expect_used,
            reason = "the primary key columns of a table without a surrogate key belong to its \
                      new record, which callers check is complete before writing"
        )]
        let key = <T::NewValues as MayGetNestedColumns<T::NestedPrimaryKeyColumns>>::may_get_nested_columns(
            &new_values,
        )
        .transpose()
        .expect("every primary key column was already proven present");
        diesel::insert_into(T::default()).values(T::insert_values(new_values)).execute(conn)?;
        Ok(key)
    }
}

/// The database assigns the key, so the insert returns it.
impl<Conn, T> WriteRow<Conn, T> for SurrogateKey
where
    T: InsertValues,
    diesel::dsl::Returning<
        InsertStatement<T, <T::Values as Insertable<T>>::Values>,
        T::NestedPrimaryKeyColumns,
    >: diesel::query_builder::Query + for<'query> LoadQuery<'query, Conn, PrimaryKeyValues<T>>,
{
    fn write_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<PrimaryKeyValues<T>> {
        diesel::insert_into(T::default())
            .values(T::insert_values(new_values))
            .returning(T::NestedPrimaryKeyColumns::default())
            .get_result(conn)
    }
}

/// The primary key values of a row of `T` as a filter takes them.
type PrimaryKeyFilter<T> =
    <<T as TableExt>::NestedPrimaryKeyColumns as TypedNestedTuple>::NestedTupleValueType;

/// A table whose stored rows load by primary key.
pub(crate) trait LoadStoredRow<Conn>: TableExt {
    /// Loads the stored row with the given primary key.
    ///
    /// # Errors
    ///
    /// Returns the database error of the query, or
    /// [`NotFound`](diesel::result::Error::NotFound) when the row is not
    /// visible.
    fn load_stored_row(
        key: PrimaryKeyValues<Self>,
        conn: &mut Conn,
    ) -> diesel::QueryResult<Self::Model>;
}

impl<Conn, T> LoadStoredRow<Conn> for T
where
    T: TableExt<NestedPrimaryKeyColumns: LoadQueryBuilder>,
    PrimaryKeyValues<T>: NestedTupleInto<PrimaryKeyFilter<T>>,
    <T::NestedPrimaryKeyColumns as LoadQueryBuilder>::LoadQuery:
        for<'query> LoadQuery<'query, Conn, T::Model>,
{
    fn load_stored_row(key: PrimaryKeyValues<T>, conn: &mut Conn) -> diesel::QueryResult<T::Model> {
        T::NestedPrimaryKeyColumns::load_query(key).get_result(conn)
    }
}

/// A Diesel backend on which a builder writes the last row of an insert into
/// `T` and reads back the model the database stored.
#[diagnostic::on_unimplemented(
    message = "diesel-builders cannot insert rows on the backend `{Self}`",
    note = "enable the `sqlite` or `postgres` feature of diesel-builders for the backend in use"
)]
pub(crate) trait InsertStoredRow<Conn, T: InsertValues> {
    /// Writes the row holding `new_values` and returns the stored row.
    ///
    /// # Errors
    ///
    /// Returns the database error of the insert or of reading the row back.
    fn insert_stored_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<T::Model>;
}

/// The last row is written once every other row exists, so `RETURNING`
/// reports it in the same statement.
#[cfg(feature = "postgres")]
impl<Conn, T> InsertStoredRow<Conn, T> for diesel::pg::Pg
where
    T: InsertValues,
    InsertStatement<T, <T::Values as Insertable<T>>::Values>:
        for<'query> LoadQuery<'query, Conn, T::Model>,
{
    fn insert_stored_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<T::Model> {
        diesel::insert_into(T::default()).values(T::insert_values(new_values)).get_result(conn)
    }
}

/// `RETURNING` on a view that `INSTEAD OF` triggers write through reports the
/// row as the statement wrote it, with `NULL` for every column the database
/// fills in, so the row is written and then loaded by its key.
#[cfg(feature = "sqlite")]
impl<Conn, T> InsertStoredRow<Conn, T> for diesel::sqlite::Sqlite
where
    T: InsertValues + LoadStoredRow<Conn>,
    T::KeySource: WriteRow<Conn, T>,
{
    fn insert_stored_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<T::Model> {
        let key = <T::KeySource as WriteRow<Conn, T>>::write_row(new_values, conn)?;
        T::load_stored_row(key, conn)
    }
}
