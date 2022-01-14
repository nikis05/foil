use futures::{future::BoxFuture, stream::BoxStream};
use sqlx::query::Query;
use sqlx::{database::HasArguments, Database, Decode, Encode, Row, Type};
use std::error::Error;
use thiserror::Error;
use vec1::Vec1;

mod display;
mod impls {
    mod executor;
    pub mod log;
    #[cfg(all(feature = "test-manager", feature = "sqlite"))]
    pub mod mock;
}

pub use impls::log::LogManager;
#[cfg(all(feature = "test-manager", feature = "sqlite"))]
pub use impls::mock::MockManager;

pub trait Manager<'m, DB: Database>: Send {
    type Error: Error + Send + 'm;

    fn select<'o, 'q>(
        self,
        query: SelectQuery<'q, DB>,
    ) -> BoxStream<'o, Result<Record<DB>, Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn count<'o, 'q>(self, query: CountQuery<'q, DB>) -> BoxFuture<'o, Result<u32, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
        for<'a> u32: Type<DB> + Decode<'a, DB>,
        for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>;

    fn insert<'o, 'q>(self, query: InsertQuery<'q, DB>) -> BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn insert_returning<'o, 'q>(
        self,
        query: InsertReturningQuery<'q, DB>,
    ) -> BoxStream<'o, Result<Record<DB>, Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn update<'o, 'q>(self, query: UpdateQuery<'q, DB>) -> BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn delete<'o, 'q>(self, query: DeleteQuery<'q, DB>) -> BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o;
}

pub struct SelectQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub col_names: Vec1<&'q str>,
    pub conds: Vec1<Condition<DB>>,
    pub order_by: Option<(Vec1<&'q str>, Order)>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

pub struct CountQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub conds: Vec1<Condition<DB>>,
}

pub struct InsertQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub col_names: Vec1<&'q str>,
    pub values: Vec1<Values<DB>>,
}

pub struct InsertReturningQuery<'q, DB: Database> {
    pub insert_query: InsertQuery<'q, DB>,
    pub returning_cols: Vec1<&'q str>,
}

pub struct UpdateQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub conds: Vec1<Condition<DB>>,
    pub new_values: Values<DB>,
}

pub struct DeleteQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub conds: Vec1<Condition<DB>>,
}

pub struct Condition<DB: Database>(Vec<(&'static str, FindOperator<Box<dyn Value<DB>>>)>);

impl<DB: Database> Default for Condition<DB> {
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<DB: Database> Condition<DB> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn cols(
        &self,
    ) -> impl ExactSizeIterator<Item = (&'static str, &FindOperator<Box<dyn Value<DB>>>)> {
        self.0
            .iter()
            .map(|(col_name, operator)| (*col_name, operator))
    }

    pub fn into_cols(
        self,
    ) -> impl ExactSizeIterator<Item = (&'static str, FindOperator<Box<dyn Value<DB>>>)> {
        self.0.into_iter()
    }

    pub fn add_col(&mut self, col_name: &'static str, operator: FindOperator<Box<dyn Value<DB>>>) {
        self.0.push((col_name, operator));
    }
}

pub trait IntoCondition<DB: Database> {
    fn into_condition(self) -> Condition<DB>;
}

pub enum FindOperator<T> {
    Eq(T),
    Ne(T),
    In(Vec1<T>),
    NotIn(Vec1<T>),
}

impl<T> FindOperator<T> {
    pub fn boxed<DB: Database>(self) -> FindOperator<Box<dyn Value<DB>>>
    where
        T: Value<DB>,
    {
        match self {
            Self::Eq(val) => FindOperator::Eq(Box::new(val)),
            Self::Ne(val) => FindOperator::Ne(Box::new(val)),
            Self::In(vals) => {
                FindOperator::In(vals.mapped(|val| Box::new(val) as Box<dyn Value<DB>>))
            }
            Self::NotIn(vals) => {
                FindOperator::NotIn(vals.mapped(|val| Box::new(val) as Box<dyn Value<DB>>))
            }
        }
    }
}

pub struct Values<DB: Database>(Vec<(&'static str, Box<dyn Value<DB>>)>);

impl<DB: Database> Values<DB> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn has_col(&self, col_name: &str) -> bool {
        self.0.iter().any(|entry| entry.0 == col_name)
    }

    pub fn add_col(&mut self, col_name: &'static str, value: Box<dyn Value<DB>>) {
        self.0.push((col_name, value));
    }

    pub fn cols(&self) -> impl ExactSizeIterator<Item = (&'static str, &Box<dyn Value<DB>>)> {
        self.0.iter().map(|(col_name, value)| (*col_name, value))
    }

    pub fn into_cols(self) -> impl ExactSizeIterator<Item = (&'static str, Box<dyn Value<DB>>)> {
        self.0
            .into_iter()
            .map(|(col_name, value)| (col_name, value))
    }
}

impl<DB: Database> Default for Values<DB> {
    fn default() -> Self {
        Self(Vec::default())
    }
}

pub trait ToValues<DB: Database> {
    fn to_values(&self) -> Values<DB>;
}

pub trait Value<DB: Database>: Send + 'static {
    fn is_null(&self) -> bool;

    fn bind<'q>(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments>
    where
        Self: 'q;
}

impl<T, DB: Database> Value<DB> for T
where
    T: for<'q> Encode<'q, DB> + Type<DB> + Send + NonNullableValue + 'static,
{
    fn is_null(&self) -> bool {
        false
    }

    fn bind<'q>(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments>
    where
        Self: 'q,
    {
        query.bind(*self)
    }
}

pub trait NonNullableValue {}

macro_rules! impl_non_nullable_value {
    ( $( $type:ty ),+ ) => {
        $(
            impl NonNullableValue for $type {}
        )+
    };
}

impl_non_nullable_value!(bool, u8, i8, u16, i16, u32, i32, f32, u64, i64, f64, String);

impl<T> NonNullableValue for Vec<T> {}

impl<T, DB: Database> Value<DB> for Option<T>
where
    Option<T>: for<'q> Encode<'q, DB> + Type<DB> + Send + 'static,
{
    fn is_null(&self) -> bool {
        self.is_none()
    }

    fn bind<'q>(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments>
    where
        Self: 'q,
    {
        query.bind(*self)
    }
}

#[cfg(feature = "bigdecimal")]
impl_non_nullable_value!(sqlx::types::BigDecimal);

#[cfg(feature = "decimal")]
impl_non_nullable_value!(sqlx::types::Decimal);

#[cfg(feature = "json")]
impl<T> NonNullableValue for sqlx::types::Json<T> {}

#[cfg(feature = "time")]
impl_non_nullable_value!(
    sqlx::types::time::Date,
    sqlx::types::time::OffsetDateTime,
    sqlx::types::time::PrimitiveDateTime,
    sqlx::types::time::Time,
    sqlx::types::time::UtcOffset
);

#[cfg(feature = "chrono")]
impl<TZ: sqlx::types::chrono::TimeZone> NonNullableValue for sqlx::types::chrono::DateTime<TZ> {}

#[cfg(feature = "chrono")]
impl_non_nullable_value!(
    sqlx::types::chrono::FixedOffset,
    sqlx::types::chrono::Local,
    sqlx::types::chrono::NaiveDate,
    sqlx::types::chrono::NaiveDateTime,
    sqlx::types::chrono::NaiveTime,
    sqlx::types::chrono::Utc
);

#[cfg(feature = "ipnetwork")]
impl_non_nullable_value!(
    sqlx::types::ipnetwork::Ipv4Network,
    sqlx::types::ipnetwork::Ipv6Network,
    sqlx::types::ipnetwork::IpNetwork
);

#[cfg(feature = "mac_address")]
impl_non_nullable_value!(sqlx::types::mac_address::MacAddress);

#[cfg(feature = "uuid")]
impl_non_nullable_value!(sqlx::types::Uuid);

#[cfg(feature = "bit-vec")]
impl_non_nullable_value!(sqlx::types::BitVec);

#[cfg(feature = "bstr")]
impl_non_nullable_value!(sqlx::types::bstr::BString);

#[cfg(feature = "git2")]
impl_non_nullable_value!(sqlx::types::git2::Oid);

pub enum Order {
    Asc,
    Desc,
}

pub struct Record<DB: Database>(DB::Row);

impl<DB: Database> Record<DB> {
    fn from_row(row: DB::Row) -> Self {
        Self(row)
    }

    pub fn col<T: sqlx::Type<DB> + for<'d> sqlx::Decode<'d, DB>>(
        &self,
        col_name: &str,
    ) -> Result<T, RecordError>
    where
        for<'a> &'a str: sqlx::ColumnIndex<DB::Row>,
    {
        self.0.try_get(col_name).map_err(|e| match e {
            sqlx::Error::ColumnNotFound(c) => RecordError::ColumnNotFound(c),
            sqlx::Error::ColumnDecode { index, source } => {
                RecordError::ColumnDecode { index, source }
            }
            _ => unreachable!(),
        })
    }
}

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("column not found: {0}")]
    ColumnNotFound(String),
    #[error("error decoding column {index}: {source}")]
    ColumnDecode {
        index: String,
        source: Box<dyn std::error::Error>,
    },
}

pub trait FromRecord<DB: Database>: Sized {
    fn from_record(record: &Record<DB>) -> Result<Self, RecordError>;
}
