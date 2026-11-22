//! Trait indicating a builder can set multiple columns.

use crate::{
    OptionalRef, SameAsReceiver, SetColumn, TableExt, TypedColumn,
    columns::{HomogeneouslyTypedNestedColumns, NonEmptyNestedProjection},
    horizontal_same_as::receive_same_as_value,
};

/// Trait indicating a builder can set multiple columns.
///
/// A `None` value sets every nullable column to `NULL` and leaves every
/// `NOT NULL` column unset, as described by [`SameAsReceiver`].
pub trait SetHomogeneousNestedColumns<Type, CS: HomogeneouslyTypedNestedColumns<Type>> {
    /// Set the `nested_values` of the specified columns.
    ///
    /// ```
    /// # include!("../doctest_setup.rs");
    /// # fn main() {
    /// use diesel_builders::{MayGetColumn, SetHomogeneousNestedColumns};
    /// use schema::intervals;
    ///
    /// let mut values = intervals::table::empty_new_values();
    /// SetHomogeneousNestedColumns::<i32, (intervals::start, (intervals::end,))>::set_homogeneous_nested_columns(
    ///     &mut values,
    ///     &None::<i32>,
    /// );
    /// assert_eq!(MayGetColumn::<intervals::start>::may_get_column(&values), None);
    /// SetHomogeneousNestedColumns::<i32, (intervals::start, (intervals::end,))>::set_homogeneous_nested_columns(
    ///     &mut values,
    ///     &3,
    /// );
    /// assert_eq!(MayGetColumn::<intervals::start>::may_get_column(&values), Some(3));
    /// assert_eq!(MayGetColumn::<intervals::end>::may_get_column(&values), Some(3));
    /// assert!(values.validate_record().is_err());
    /// # }
    /// ```
    fn set_homogeneous_nested_columns(&mut self, value: &impl OptionalRef<Type>) -> &mut Self;
}

impl<Type, T> SetHomogeneousNestedColumns<Type, ()> for T {
    #[inline]
    fn set_homogeneous_nested_columns(&mut self, _value: &impl OptionalRef<Type>) -> &mut Self {
        self
    }
}

impl<Type: Clone + Into<C1::ValueType>, C1, T> SetHomogeneousNestedColumns<Type, (C1,)> for T
where
    T: SetColumn<C1>,
    C1: TypedColumn<Table: TableExt>,
    C1::ColumnType: SameAsReceiver<C1::ValueType>,
{
    #[inline]
    fn set_homogeneous_nested_columns(&mut self, value: &impl OptionalRef<Type>) -> &mut Self {
        if let Some(value) = receive_same_as_value::<Type, C1>(value) {
            self.set_column(value);
        }
        self
    }
}

impl<Type: Clone + Into<CHead::ValueType>, CHead, CTail, T>
    SetHomogeneousNestedColumns<Type, (CHead, CTail)> for T
where
    CHead: TypedColumn,
    CTail: HomogeneouslyTypedNestedColumns<Type>,
    CHead::ColumnType: SameAsReceiver<CHead::ValueType>,
    (CHead, CTail): NonEmptyNestedProjection<
        NestedTupleValueType = (CHead::ValueType, CTail::NestedTupleValueType),
    >,
    T: SetColumn<CHead> + SetHomogeneousNestedColumns<Type, CTail>,
{
    #[inline]
    fn set_homogeneous_nested_columns(&mut self, value: &impl OptionalRef<Type>) -> &mut Self {
        self.set_homogeneous_nested_columns(value);
        if let Some(value) = receive_same_as_value::<Type, CHead>(value) {
            self.set_column(value);
        }
        self
    }
}
