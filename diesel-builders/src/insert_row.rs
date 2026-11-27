//! The `INSERT` statement a builder writes for one row.
//!
//! The statement names only the columns whose value the builder holds, so a
//! column the database supplies stays out of the column list until the caller
//! sets it. PostgreSQL requires the `INSERT` privilege on every column a
//! statement names, even one written as `DEFAULT`, so leaving the column out
//! is the only way a role without that privilege can insert the row.

use core::marker::PhantomData;

use diesel::{
    Column, Expression, QueryResult, RunQueryDsl,
    backend::Backend,
    dsl::AsExprOf,
    expression::AsExpression,
    query_builder::{AstPass, Query, QueryFragment, QueryId},
    query_source::QuerySource,
    sql_types::SingleValue,
};

use crate::TypedColumn;

/// How an insert treats a column of a table's new record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InsertPolicy {
    /// The builder must hold a value, which the insert writes.
    Required,
    /// The insert writes the value when the builder holds one, and otherwise
    /// leaves the column out so the database supplies it.
    DatabaseDefault,
    /// The database generates the column as a constant, which the builder
    /// holds so a primary key including it is known before the insert, and
    /// which the insert never writes.
    Generated,
}

/// A column of a table's new record.
pub trait NewRecordColumn: TypedColumn {
    /// How an insert treats the column.
    const INSERT_POLICY: InsertPolicy;
}

/// The value an insert writes into column `C`, absent when the builder holds
/// none.
pub(crate) struct ColumnValue<C, Value> {
    /// The value, or `None` to leave the column out of the statement.
    value: Option<Value>,
    /// The column the value belongs to.
    column: PhantomData<C>,
}

/// The values of one row, in column order, of which an insert writes the
/// present ones.
pub(crate) trait RowValues<DB: Backend> {
    /// Whether the row holds no value at all.
    fn is_empty(&self) -> bool;

    /// Writes the names of the columns holding a value, preceded by a comma
    /// when `separated`, and returns whether a name precedes what follows.
    ///
    /// # Errors
    ///
    /// Returns the error of writing an identifier.
    fn walk_names(&self, out: AstPass<'_, '_, DB>, separated: bool) -> QueryResult<bool>;

    /// Writes the present values, preceded by a comma when `separated`, and
    /// returns whether a value precedes what follows.
    ///
    /// # Errors
    ///
    /// Returns the error of writing a value.
    fn walk_values<'b>(&'b self, out: AstPass<'_, 'b, DB>, separated: bool) -> QueryResult<bool>;
}

impl<C, Value, DB> RowValues<DB> for ColumnValue<C, Value>
where
    C: Column,
    Value: QueryFragment<DB>,
    DB: Backend,
{
    fn is_empty(&self) -> bool {
        self.value.is_none()
    }

    fn walk_names(&self, mut out: AstPass<'_, '_, DB>, separated: bool) -> QueryResult<bool> {
        if self.value.is_none() {
            return Ok(separated);
        }
        if separated {
            out.push_sql(", ");
        }
        out.push_identifier(C::NAME)?;
        Ok(true)
    }

    fn walk_values<'b>(
        &'b self,
        mut out: AstPass<'_, 'b, DB>,
        separated: bool,
    ) -> QueryResult<bool> {
        let Some(value) = &self.value else {
            return Ok(separated);
        };
        if separated {
            out.push_sql(", ");
        }
        value.walk_ast(out)?;
        Ok(true)
    }
}

impl<Head, DB> RowValues<DB> for (Head,)
where
    Head: RowValues<DB>,
    DB: Backend,
{
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn walk_names(&self, out: AstPass<'_, '_, DB>, separated: bool) -> QueryResult<bool> {
        self.0.walk_names(out, separated)
    }

    fn walk_values<'b>(&'b self, out: AstPass<'_, 'b, DB>, separated: bool) -> QueryResult<bool> {
        self.0.walk_values(out, separated)
    }
}

impl<Head, Tail, DB> RowValues<DB> for (Head, Tail)
where
    Head: RowValues<DB>,
    Tail: RowValues<DB>,
    DB: Backend,
{
    fn is_empty(&self) -> bool {
        self.0.is_empty() && self.1.is_empty()
    }

    fn walk_names(&self, mut out: AstPass<'_, '_, DB>, separated: bool) -> QueryResult<bool> {
        let separated = self.0.walk_names(out.reborrow(), separated)?;
        self.1.walk_names(out, separated)
    }

    fn walk_values<'b>(
        &'b self,
        mut out: AstPass<'_, 'b, DB>,
        separated: bool,
    ) -> QueryResult<bool> {
        let separated = self.0.walk_values(out.reborrow(), separated)?;
        self.1.walk_values(out, separated)
    }
}

/// The nested columns of a table's new record, and the row an insert writes
/// from the values a builder holds for them.
pub(crate) trait NewRecordColumns {
    /// One optional value per column, as a builder holds them.
    type Values;
    /// The row an insert writes.
    type Row;

    /// Returns the name of the first column that holds no value and that the
    /// database does not supply.
    fn first_missing(values: &Self::Values) -> Option<&'static str>;

    /// Converts the values a builder holds into the row an insert writes.
    fn row(values: Self::Values) -> Self::Row;
}

/// The insert value of column `C`.
type ColumnExpression<C> =
    AsExprOf<<C as crate::ColumnTyped>::ColumnType, <C as Expression>::SqlType>;

/// Converts the value a builder holds for column `C` into its insert value,
/// absent for a column the database generates.
fn column_value<C>(value: Option<C::ColumnType>) -> ColumnValue<C, ColumnExpression<C>>
where
    C: NewRecordColumn<ColumnType: AsExpression<C::SqlType>> + Expression<SqlType: SingleValue>,
{
    let value = value.filter(|_| C::INSERT_POLICY != InsertPolicy::Generated);
    ColumnValue { value: value.map(AsExpression::as_expression), column: PhantomData }
}

/// Returns the name of `C` when it holds no value and the database does not
/// supply it.
fn missing_column<C: NewRecordColumn>(value: Option<&C::ColumnType>) -> Option<&'static str> {
    (value.is_none() && C::INSERT_POLICY != InsertPolicy::DatabaseDefault).then_some(C::NAME)
}

impl<C> NewRecordColumns for (C,)
where
    C: NewRecordColumn<ColumnType: AsExpression<C::SqlType>> + Expression<SqlType: SingleValue>,
{
    type Values = (Option<C::ColumnType>,);
    type Row = (ColumnValue<C, ColumnExpression<C>>,);

    fn first_missing(values: &Self::Values) -> Option<&'static str> {
        missing_column::<C>(values.0.as_ref())
    }

    fn row((value,): Self::Values) -> Self::Row {
        (column_value::<C>(value),)
    }
}

impl<C, Tail> NewRecordColumns for (C, Tail)
where
    C: NewRecordColumn<ColumnType: AsExpression<C::SqlType>> + Expression<SqlType: SingleValue>,
    Tail: NewRecordColumns,
{
    type Values = (Option<C::ColumnType>, Tail::Values);
    type Row = (ColumnValue<C, ColumnExpression<C>>, Tail::Row);

    fn first_missing(values: &Self::Values) -> Option<&'static str> {
        missing_column::<C>(values.0.as_ref()).or_else(|| Tail::first_missing(&values.1))
    }

    fn row((value, tail): Self::Values) -> Self::Row {
        (column_value::<C>(value), Tail::row(tail))
    }
}

/// The `RETURNING` clause of an insert that reports nothing back.
pub(crate) struct NoReturning;

/// The `RETURNING` clause of an insert that reports `Columns` back.
pub(crate) struct Returning<Columns>(Columns);

impl<DB: Backend> QueryFragment<DB> for NoReturning {
    fn walk_ast<'b>(&'b self, _out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        Ok(())
    }
}

impl<Columns, DB> QueryFragment<DB> for Returning<Columns>
where
    Columns: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        out.push_sql(" RETURNING ");
        self.0.walk_ast(out)
    }
}

/// An `INSERT` of one row into table `T`.
pub(crate) struct InsertRow<T: QuerySource, Row, Ret = NoReturning> {
    /// The table the row goes into.
    into_clause: T::FromClause,
    /// The values of the row.
    row: Row,
    /// The `RETURNING` clause.
    returning: Ret,
}

impl<T: QuerySource + Default, Row> InsertRow<T, Row> {
    /// Inserts `row` into `T`, reporting nothing back.
    pub(crate) fn new(row: Row) -> Self {
        Self { into_clause: T::default().from_clause(), row, returning: NoReturning }
    }

    /// Reports `columns` of the inserted row back.
    pub(crate) fn returning<Columns>(
        self,
        columns: Columns,
    ) -> InsertRow<T, Row, Returning<Columns>> {
        InsertRow { into_clause: self.into_clause, row: self.row, returning: Returning(columns) }
    }
}

impl<T, Row, Ret, DB> QueryFragment<DB> for InsertRow<T, Row, Ret>
where
    T: QuerySource<FromClause: QueryFragment<DB>>,
    Row: RowValues<DB>,
    Ret: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        out.push_sql("INSERT INTO ");
        self.into_clause.walk_ast(out.reborrow())?;
        if self.row.is_empty() {
            out.push_sql(" DEFAULT VALUES");
        } else {
            out.push_sql(" (");
            self.row.walk_names(out.reborrow(), false)?;
            out.push_sql(") VALUES (");
            self.row.walk_values(out.reborrow(), false)?;
            out.push_sql(")");
        }
        self.returning.walk_ast(out)
    }
}

/// The statement text depends on which columns hold a value, so prepared
/// statements are cached by their text.
impl<T: QuerySource, Row, Ret> QueryId for InsertRow<T, Row, Ret> {
    type QueryId = ();
    const HAS_STATIC_QUERY_ID: bool = false;
}

impl<T, Row, Columns> Query for InsertRow<T, Row, Returning<Columns>>
where
    T: QuerySource,
    Columns: Expression,
{
    type SqlType = Columns::SqlType;
}

impl<T: QuerySource, Row, Ret, Conn> RunQueryDsl<Conn> for InsertRow<T, Row, Ret> {}
