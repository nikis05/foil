use sqlx::{database::HasArguments, query::Query, Database, Encode, Type};
use std::any::Any;

pub trait Value<'q, DB: Database>: Send {
    fn bind(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments>;

    fn is_null(&self) -> bool;

    fn to_owned_any(&self) -> Box<dyn Any>;
}

macro_rules! impl_value {
    ( $( $type:ty ),+ ) => {
        $(
            impl<'q, DB: Database> Value<'q, DB> for $type
            where
                $type: Type<DB> + Encode<'q, DB>,
            {
                fn bind(
                    self: Box<Self>,
                    query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
                ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                    query.bind(*self)
                }

                fn is_null(&self) -> bool {
                    false
                }

                #[allow(clippy::clone_on_copy)]
                fn to_owned_any(&self) -> Box<dyn Any> {
                    Box::new(self.clone())
                }
            }
        )+
    };
}

macro_rules! impl_value_generic {
    ( <$generic:ident> $type:ty where $( $where_clause:tt )+ ) => {
        impl<'q, DB: Database, $generic> Value<'q, DB> for $type
        where
            $(
                $where_clause
            )+
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }

            fn is_null(&self) -> bool {
                false
            }

            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
    };
}

macro_rules! impl_value_for_u8_slice {
    ( $db:ty ) => {
        impl<'q> Value<'q, $db> for &'q [u8] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, $db, <$db as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, $db, <$db as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }

            fn is_null(&self) -> bool {
                false
            }

            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
    };
}

macro_rules! impl_value_for_pg_array {
    ( $( $type:ty ),+ ) => {
        $(
            impl<'q> Value<'q, sqlx::Postgres> for &'q [$type] {
                fn bind(
                    self: Box<Self>,
                    query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
                ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
                    query.bind(*self)
                }

                fn is_null(&self) -> bool {
                    false
                }

                fn to_owned_any(&self) -> Box<dyn Any> {
                    Box::new(self.to_vec())
                }
            }

            impl<'q> Value<'q, sqlx::Postgres> for Vec<$type> {
                fn bind(
                    self: Box<Self>,
                    query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
                ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
                    query.bind(*self)
                }

                fn is_null(&self) -> bool {
                    false
                }

                fn to_owned_any(&self) -> Box<dyn Any> {
                    Box::new(self.clone())
                }
            }
        )+
    };
}

macro_rules! impl_value_for_pg_array_generic {
    ( <$generic:ident> $type:ty $( where $( $where_clause:tt )+ )? ) => {
        impl<'q, $generic> Value<'q, sqlx::Postgres> for &'q [$type]
        $(
            where $(
                $where_clause
            )+
        )?
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }

            fn is_null(&self) -> bool {
                false
            }

            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }

        impl<'q, $generic> Value<'q, sqlx::Postgres> for Vec<$type>
        $(
            where $(
                $where_clause
            )+
        )?
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }

            fn is_null(&self) -> bool {
                false
            }

            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
    };
}

impl<'q, DB: Database, T> Value<'q, DB> for &'q T
where
    T: Value<'q, DB> + Type<DB> + Encode<'q, DB> + Sync,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        T::is_null(*self)
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        T::to_owned_any(self)
    }
}

#[cfg(feature = "mysql")]
impl_value_for_u8_slice!(sqlx::MySql);

#[cfg(feature = "postgres")]
impl_value_for_u8_slice!(sqlx::Postgres);

#[cfg(feature = "sqlite")]
impl_value_for_u8_slice!(sqlx::Sqlite);

#[cfg(feature = "postgres")]
impl<'q, 'o> Value<'q, sqlx::Postgres> for &'o [&'q [u8]]
where
    'o: 'q,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(self.iter().map(|bytes| bytes.to_vec()).collect::<Vec<_>>())
    }
}

#[cfg(feature = "postgres")]
impl<'q> Value<'q, sqlx::Postgres> for Vec<&'q [u8]> {
    fn bind(
        self: Box<Self>,
        query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(self.iter().map(|bytes| bytes.to_vec()).collect::<Vec<_>>())
    }
}

impl<'q, DB: Database> Value<'q, DB> for &'q str
where
    &'q str: Type<DB> + Encode<'q, DB>,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new((*self).to_owned())
    }
}

#[cfg(feature = "postgres")]
impl<'q, 'o> Value<'q, sqlx::Postgres> for &'o [&'q str]
where
    'o: 'q,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(self.iter().map(|str| (*str).to_owned()).collect::<Vec<_>>())
    }
}

#[cfg(feature = "postgres")]
impl<'q> Value<'q, sqlx::Postgres> for Vec<&'q str> {
    fn bind(
        self: Box<Self>,
        query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(self.iter().map(|str| (*str).to_owned()).collect::<Vec<_>>())
    }
}

impl<'q, DB: Database> Value<'q, DB> for std::borrow::Cow<'q, str>
where
    std::borrow::Cow<'q, str>: Type<DB> + Encode<'q, DB>,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(self.clone().into_owned())
    }
}

#[cfg(feature = "postgres")]
impl<'q, 'o> Value<'q, sqlx::Postgres> for &'o [std::borrow::Cow<'q, str>]
where
    'o: 'q,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
    where
        'q: 'q,
    {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(
            self.iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
        )
    }
}

#[cfg(feature = "postgres")]
impl<'q> Value<'q, sqlx::Postgres> for Vec<std::borrow::Cow<'q, str>> {
    fn bind(
        self: Box<Self>,
        query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
    where
        'q: 'q,
    {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(
            self.iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
        )
    }
}

impl<'q, DB: Database, T> Value<'q, DB> for Option<T>
where
    T: Value<'q, DB>,
    Option<T>: Type<DB> + Encode<'q, DB> + 'q + Send,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        self.is_none()
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new(self.as_ref().map(Value::to_owned_any))
    }
}

#[cfg(feature = "postgres")]
impl_value_for_pg_array_generic!(
    <T> Option<T>
    where
      Option<T>: Type<sqlx::Postgres> + for<'e> Encode<'e, sqlx::Postgres>,
        T: Send + Sync + Clone + 'static + sqlx::postgres::PgHasArrayType
);

impl_value!(
    bool,
    u8,
    u16,
    u32,
    u64,
    i8,
    i16,
    i32,
    i64,
    f32,
    f64,
    String,
    std::time::Duration,
    Vec<u8>
);

#[cfg(feature = "postgres")]
impl_value_for_pg_array!(
    bool,
    u32,
    i8,
    i16,
    i32,
    i64,
    f32,
    f64,
    String,
    std::time::Duration,
    Vec<u8>
);

#[cfg(feature = "bigdecimal")]
impl_value!(sqlx::types::BigDecimal);

#[cfg(all(feature = "bigdecimal", feature = "postgres"))]
impl_value_for_pg_array!(sqlx::types::BigDecimal);

#[cfg(feature = "decimal")]
impl_value!(sqlx::types::Decimal);

#[cfg(all(feature = "decimal", feature = "postgres"))]
impl_value_for_pg_array!(sqlx::types::Decimal);

#[cfg(feature = "json")]
impl_value!(serde_json::Value);

#[cfg(feature = "json")]
impl_value_generic!(
    <T> sqlx::types::Json<T>
    where
        T: serde::Serialize,
        sqlx::types::Json<T>: Type<DB> + for<'e> Encode<'e, DB> + Send + 'static + Clone,
);

#[cfg(all(feature = "json", feature = "postgres"))]
impl_value_for_pg_array_generic!(
    <T> sqlx::types::Json<T>
    where
        T: serde::Serialize,
        sqlx::types::Json<T>: Type<sqlx::Postgres> + for<'e> Encode<'e, sqlx::Postgres> + Send + Sync + 'static + Clone,
);

#[cfg(feature = "time")]
impl_value!(
    sqlx::types::time::OffsetDateTime,
    sqlx::types::time::PrimitiveDateTime,
    sqlx::types::time::Time,
    sqlx::types::time::UtcOffset,
    time_rs::Duration
);

#[cfg(all(feature = "time", feature = "postgres"))]
impl_value_for_pg_array!(
    sqlx::types::time::OffsetDateTime,
    sqlx::types::time::PrimitiveDateTime,
    sqlx::types::time::Time
);

#[cfg(feature = "chrono")]
impl_value!(
    sqlx::types::chrono::FixedOffset,
    sqlx::types::chrono::Local,
    sqlx::types::chrono::NaiveDate,
    sqlx::types::chrono::NaiveTime,
    sqlx::types::chrono::NaiveDateTime,
    sqlx::types::chrono::Utc,
    chrono_rs::Duration
);

#[cfg(all(feature = "chrono", feature = "postgres"))]
impl_value_for_pg_array!(
    sqlx::types::chrono::NaiveDate,
    sqlx::types::chrono::NaiveTime,
    sqlx::types::chrono::NaiveDateTime,
    chrono_rs::Duration
);

#[cfg(feature = "chrono")]
impl_value_generic!(
    <Tz> sqlx::types::chrono::DateTime<Tz>
    where
        Tz: sqlx::types::chrono::TimeZone,
        sqlx::types::chrono::DateTime<Tz>: Type<DB> + for<'e> Encode<'e, DB> + Send + 'static
);

#[cfg(all(feature = "chrono", feature = "postgres"))]
impl_value_for_pg_array_generic!(
    <Tz> sqlx::types::chrono::DateTime<Tz>
    where
        Tz: sqlx::types::chrono::TimeZone,
        <Tz as sqlx::types::chrono::TimeZone>::Offset: Sync,
        sqlx::types::chrono::DateTime<Tz>: Type<sqlx::Postgres> + for<'e> Encode<'e, sqlx::Postgres> + Send + 'static
);

#[cfg(feature = "ipnetwork")]
impl_value!(sqlx::types::ipnetwork::IpNetwork);

#[cfg(all(feature = "ipnetwork", feature = "postgres"))]
impl_value_for_pg_array!(sqlx::types::ipnetwork::IpNetwork);

#[cfg(feature = "mac_address")]
impl_value!(sqlx::types::mac_address::MacAddress);

#[cfg(all(feature = "mac_address", feature = "postgres"))]
impl_value_for_pg_array!(sqlx::types::mac_address::MacAddress);

#[cfg(feature = "uuid")]
impl_value!(
    sqlx::types::uuid::Uuid,
    sqlx::types::uuid::adapter::Hyphenated
);

#[cfg(all(feature = "uuid", feature = "postgres"))]
impl_value_for_pg_array!(sqlx::types::uuid::Uuid);

#[cfg(feature = "bit-vec")]
impl_value!(sqlx::types::BitVec<u32>);

#[cfg(all(feature = "bit-vec", feature = "postgres"))]
impl_value_for_pg_array!(sqlx::types::BitVec<u32>);

#[cfg(feature = "bstr")]
impl<'q, DB: Database> Value<'q, DB> for &'q sqlx::types::bstr::BStr
where
    for<'e> &'e sqlx::types::bstr::BStr: Type<DB> + Encode<'e, DB>,
{
    fn bind(
        self: Box<Self>,
        query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
    ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments>
    where
        'q: 'q,
    {
        query.bind(*self)
    }

    fn is_null(&self) -> bool {
        false
    }

    fn to_owned_any(&self) -> Box<dyn Any> {
        Box::new((*self).to_owned())
    }
}

#[cfg(feature = "bstr")]
impl_value!(sqlx::types::bstr::BString);

#[cfg(feature = "git2")]
impl_value!(sqlx::types::git2::Oid);

#[cfg(feature = "postgres")]
impl_value!(
    sqlx::postgres::types::PgInterval,
    sqlx::postgres::types::PgLQuery,
    sqlx::postgres::types::PgLTree,
    sqlx::postgres::types::PgMoney
);

#[cfg(feature = "postgres")]
impl_value_for_pg_array!(
    sqlx::postgres::types::PgInterval,
    sqlx::postgres::types::PgLTree,
    sqlx::postgres::types::PgMoney
);

#[cfg(all(feature = "postgres", feature = "chrono"))]
impl_value!(sqlx::postgres::types::PgTimeTz<sqlx::types::chrono::NaiveTime, sqlx::types::chrono::FixedOffset>);

#[cfg(all(feature = "postgres", feature = "time"))]
impl_value!(sqlx::postgres::types::PgTimeTz<sqlx::types::time::Time, sqlx::types::time::UtcOffset>);
