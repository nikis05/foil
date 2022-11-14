use super::{
    CountQuery, DeleteQuery, FindOperator, InsertQuery, InsertReturningQuery, Order, SelectQuery,
    Selector, UpdateQuery, Value,
};
use sqlx::Database;
use std::fmt::{Display, Formatter, Result};

#[cfg(all(test, feature = "postgres", feature = "sqlite", feature = "mssql"))]
mod test;

impl<DB: Database + WithBindParameters> Display for SelectQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "SELECT ")?;

        format_list(self.col_names.iter(), f, |col_name, f| {
            write!(f, "\"{}\"", col_name)
        })?;

        write!(f, " FROM \"{}\"", self.table_name)?;

        format_selectors(&self.selectors, &mut DB::parameter_factory(), f)?;

        if let Some(order_by) = &self.order_by {
            write!(f, " ORDER BY ")?;

            format_list(order_by.cols.iter(), f, |col_name, f| {
                write!(f, "\"{}\"", col_name)
            })?;

            write!(f, " {}", order_by.order)?;
        }

        if let Some(offset) = self.offset {
            write!(f, " OFFSET {}", offset)?;
        }

        if let Some(limit) = self.limit {
            write!(f, " LIMIT {}", limit)?;
        }

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for CountQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "SELECT COUNT (*) AS \"cnt\" FROM \"{}\"",
            self.table_name
        )?;

        format_selectors(&self.selectors, &mut DB::parameter_factory(), f)?;

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for InsertQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "INSERT INTO \"{}\" (", self.table_name)?;

        let col_names = self
            .values
            .iter()
            .flat_map(|input_record| input_record.cols().map(|(col_name, _)| col_name.to_owned()))
            .fold(Vec::new(), |mut col_names, col_name| {
                if !col_names.contains(&col_name) {
                    col_names.push(col_name);
                }
                col_names
            });

        format_list(col_names.iter(), f, |col_name, f| {
            write!(f, "\"{}\"", col_name)
        })?;

        write!(f, ") VALUES ")?;

        let mut parameter_factory = DB::parameter_factory();

        format_list(self.values.iter(), f, |values, f| {
            write!(f, "(")?;

            format_list(col_names.iter(), f, |col_name, f| {
                if values.has_col(col_name) {
                    write!(f, "{}", parameter_factory.get())?;
                } else {
                    write!(f, "DEFAULT")?;
                }

                Ok(())
            })?;

            write!(f, ")")?;

            Ok(())
        })?;

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for InsertReturningQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.insert_query)?;

        if self.returning_cols.is_empty() {
            return Ok(());
        }

        write!(f, " RETURNING ")?;

        format_list(self.returning_cols.iter(), f, |col_name, f| {
            write!(f, "\"{}\"", col_name)
        })?;

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for UpdateQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "UPDATE \"{}\" SET ", self.table_name)?;

        let mut parameter_factory = DB::parameter_factory();
        let cols = self.new_values.cols();

        format_list(cols, f, |(col_name, _), f| {
            write!(f, "\"{}\" = {}", col_name, parameter_factory.get())
        })?;

        format_selectors(&self.selectors, &mut parameter_factory, f)?;

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for DeleteQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "DELETE FROM \"{}\"", self.table_name)?;
        format_selectors(&self.selectors, &mut DB::parameter_factory(), f)
    }
}

impl Display for Order {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str(match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        })
    }
}

fn format_selectors<DB: Database + WithBindParameters, W: std::fmt::Write>(
    selectors: &[Selector<DB>],
    parameter_factory: &mut DB::ParameterFactory,
    w: &mut W,
) -> Result {
    if selectors.len() == 1 && selectors[0].is_empty() {
        return Ok(());
    }

    write!(w, " WHERE ")?;

    match selectors.len() {
        0 => write!(w, "<empty list>")?,
        1 => format_selector(selectors.first().unwrap(), parameter_factory, w)?,
        _ => {
            for (index, selector) in selectors.iter().enumerate() {
                write!(w, "(")?;
                format_selector(selector, parameter_factory, w)?;
                write!(w, ")")?;

                if index != selectors.len() - 1 {
                    write!(w, " OR ")?;
                }
            }
        }
    }

    Ok(())
}

fn format_selector<DB: Database + WithBindParameters, W: std::fmt::Write>(
    selector: &Selector<DB>,
    parameter_factory: &mut DB::ParameterFactory,
    f: &mut W,
) -> Result {
    let mut format_col =
        |f: &mut W, (col_name, op): (&str, &FindOperator<Box<dyn Value<DB>>>)| match op {
            FindOperator::Eq(value) => {
                if value.is_null() {
                    write!(f, "\"{}\" IS NULL", col_name)
                } else {
                    write!(f, "\"{}\" = {}", col_name, parameter_factory.get())
                }
            }
            FindOperator::Ne(value) => {
                if value.is_null() {
                    write!(f, "\"{}\" IS NOT NULL", col_name)
                } else {
                    write!(f, "\"{}\" != {}", col_name, parameter_factory.get())
                }
            }
            FindOperator::In(values) => {
                write!(f, "\"{}\" IN (", col_name)?;

                format_list(
                    values
                        .iter()
                        .filter_map(|value| {
                            if value.is_null() {
                                None
                            } else {
                                Some(parameter_factory.get())
                            }
                        })
                        .collect::<Vec<_>>()
                        .iter(),
                    f,
                    |parameter, f| write!(f, "{}", parameter),
                )?;

                write!(f, ")")?;
                if values.iter().any(|value| value.is_null()) {
                    write!(f, " OR \"{}\" IS NULL", col_name)?;
                }

                Ok(())
            }
            FindOperator::NotIn(values) => {
                write!(f, "\"{}\" NOT IN (", col_name)?;

                format_list(
                    values
                        .iter()
                        .filter_map(|value| {
                            if value.is_null() {
                                None
                            } else {
                                Some(parameter_factory.get())
                            }
                        })
                        .collect::<Vec<_>>()
                        .iter(),
                    f,
                    |parameter, f| write!(f, "{}", parameter),
                )?;

                write!(f, ")")?;

                if values.iter().any(|value| value.is_null()) {
                    write!(f, " AND \"{}\" IS NOT NULL", col_name)?;
                }

                Ok(())
            }
        };

    match selector.len() {
        0 => write!(f, "<empty list>")?,
        1 => format_col(f, selector.cols().next().unwrap())?,
        _ => {
            for (index, col) in selector.cols().enumerate() {
                write!(f, "(")?;
                format_col(f, col)?;
                write!(f, ")")?;

                if index != selector.cols().len() - 1 {
                    write!(f, " AND ")?;
                }
            }
        }
    }

    Ok(())
}

fn format_list<T, W: std::fmt::Write>(
    list: impl ExactSizeIterator<Item = T>,
    f: &mut W,
    mut format_fn: impl FnMut(T, &mut W) -> Result,
) -> Result {
    let len = list.len();

    if len == 0 {
        write!(f, "<empty list>")?;
        return Ok(());
    }

    for (index, item) in list.enumerate() {
        format_fn(item, f)?;
        if index != len - 1 {
            write!(f, ", ")?;
        }
    }

    Ok(())
}

pub trait WithBindParameters {
    type ParameterFactory: ParameterFactory;

    fn parameter_factory() -> Self::ParameterFactory {
        Self::ParameterFactory::default()
    }
}

pub trait ParameterFactory: Default {
    fn get(&mut self) -> String;
}

#[cfg(feature = "postgres")]
mod pg_parameters {
    use super::{ParameterFactory, WithBindParameters};

    impl WithBindParameters for sqlx::Postgres {
        type ParameterFactory = PgParameterFactory;
    }

    #[derive(Default)]
    pub struct PgParameterFactory(usize);

    impl ParameterFactory for PgParameterFactory {
        fn get(&mut self) -> String {
            self.0 += 1;
            format!("${}", self.0)
        }
    }
}

#[cfg(any(feature = "mysql", feature = "sqlite", feature = "any"))]
mod unordered_parameters {
    use super::{ParameterFactory, WithBindParameters};

    #[cfg(feature = "mysql")]
    impl WithBindParameters for sqlx::MySql {
        type ParameterFactory = UnorderedParameterFactory;
    }

    #[cfg(feature = "sqlite")]
    impl WithBindParameters for sqlx::Sqlite {
        type ParameterFactory = UnorderedParameterFactory;
    }

    #[cfg(feature = "any")]
    impl WithBindParameters for sqlx::Any {
        type ParameterFactory = UnorderedParameterFactory;
    }

    #[derive(Default)]
    pub struct UnorderedParameterFactory;

    impl ParameterFactory for UnorderedParameterFactory {
        fn get(&mut self) -> String {
            "?".into()
        }
    }
}

#[cfg(feature = "mssql")]
mod mssql_parameters {
    use super::{ParameterFactory, WithBindParameters};

    impl WithBindParameters for sqlx::Mssql {
        type ParameterFactory = MssqlParameterFactory;
    }

    #[derive(Default)]
    pub struct MssqlParameterFactory(usize);

    impl ParameterFactory for MssqlParameterFactory {
        fn get(&mut self) -> String {
            self.0 += 1;
            format!("@P{}", self.0)
        }
    }
}
