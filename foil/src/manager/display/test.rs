use crate::manager::{
    display::{format_conds, WithBindParameters},
    Condition, CountQuery, DeleteQuery, FindOperator, InsertQuery, InsertReturningQuery, Order,
    SelectQuery, UpdateQuery, Values,
};
use insta::assert_snapshot;
use sqlx::{Mssql, Postgres, Sqlite};
use vec1::vec1;

mod conds {
    use crate::manager::Value;

    use super::*;

    #[test]
    fn empty_cond() {
        let mut output = String::new();

        let mut cond1 = Condition::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));
        let cond2 = Condition::default();

        format_conds(
            &[cond1, cond2],
            &mut Postgres::parameter_factory(),
            &mut output,
        )
        .unwrap();

        assert_snapshot!(output, @"");
    }

    #[test]
    fn eq() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col", FindOperator::Eq(Box::new(1)));

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" = $1"###);
    }

    #[test]
    fn eq_null() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col", FindOperator::Eq(Box::new(Option::<i32>::None)));

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IS NULL"###);
    }

    #[test]
    fn ne() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col", FindOperator::Ne(Box::new(1)));

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" != $1"###);
    }

    #[test]
    fn ne_null() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col", FindOperator::Ne(Box::new(Option::<i32>::None)));

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IS NOT NULL"###);
    }

    #[test]
    fn in_() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col(
            "col",
            FindOperator::In(vec1![
                Box::new(1) as Box<dyn Value<Postgres>>,
                Box::new(2),
                Box::new(3)
            ]),
        );

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN ($1, $2, $3)"###);
    }

    #[test]
    fn in_with_null() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col(
            "col",
            FindOperator::In(vec1![
                Box::new(Some(1)) as Box<dyn Value<Postgres>>,
                Box::new(Some(2)),
                Box::new(Option::<i32>::None),
                Box::new(Option::<i32>::None),
            ]),
        );

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN ($1, $2) OR "col" IS NULL"###);
    }

    #[test]
    fn not_in() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col(
            "col",
            FindOperator::NotIn(vec1![
                Box::new(1) as Box<dyn Value<Postgres>>,
                Box::new(2),
                Box::new(3)
            ]),
        );

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" NOT IN ($1, $2, $3)"###);
    }

    #[test]
    fn not_in_with_null() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col(
            "col",
            FindOperator::NotIn(vec1![
                Box::new(Some(1)) as Box<dyn Value<Postgres>>,
                Box::new(Some(2)),
                Box::new(Option::<i32>::None),
                Box::new(Option::<i32>::None),
            ]),
        );

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" NOT IN ($1, $2) AND "col" IS NOT NULL"###);
    }

    #[test]
    fn multiple_cols() {
        let mut output = String::new();

        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col1", FindOperator::Eq(Box::new(1)));
        cond.add_col("col2", FindOperator::Eq(Box::new(2)));

        format_conds(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE ("col1" = $1) AND ("col2" = $2)"###);
    }

    #[test]
    fn multiple_conds() {
        let mut output = String::new();

        let mut cond1 = Condition::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));

        let mut cond2 = Condition::default();
        cond2.add_col("col2", FindOperator::Eq(Box::new(2)));

        format_conds(
            &[cond1, cond2],
            &mut Postgres::parameter_factory(),
            &mut output,
        )
        .unwrap();

        assert_snapshot!(output, @r###" WHERE ("col1" = $1) OR ("col2" = $2)"###);
    }

    #[test]
    fn sqlite_mysql_params() {
        let mut output = String::new();

        let mut cond = Condition::<Sqlite>::default();
        cond.add_col(
            "col",
            FindOperator::In(vec1![
                Box::new(1) as Box<dyn Value<Sqlite>>,
                Box::new(2),
                Box::new(3)
            ]),
        );

        format_conds(&[cond], &mut Sqlite::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN (?, ?, ?)"###);
    }

    #[test]
    fn mssql_params() {
        let mut output = String::new();

        let mut cond = Condition::<Mssql>::default();
        cond.add_col(
            "col",
            FindOperator::In(vec1![
                Box::new(1) as Box<dyn Value<Mssql>>,
                Box::new(2),
                Box::new(3)
            ]),
        );

        format_conds(&[cond], &mut Mssql::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN (@P1, @P2, @P3)"###);
    }

    #[test]
    fn combined() {
        let mut output = String::new();

        let mut cond1 = Condition::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));
        cond1.add_col("col2", FindOperator::Eq(Box::new(Option::<i32>::None)));
        cond1.add_col(
            "col3",
            FindOperator::In(vec1![
                Box::new(2) as Box<dyn Value<Postgres>>,
                Box::new(Option::<i32>::None)
            ]),
        );

        let mut cond2 = Condition::<Postgres>::default();
        cond2.add_col("col1", FindOperator::Ne(Box::new(1)));
        cond2.add_col("col2", FindOperator::Ne(Box::new(Option::<i32>::None)));
        cond2.add_col(
            "col3",
            FindOperator::NotIn(vec1![
                Box::new(2) as Box<dyn Value<Postgres>>,
                Box::new(Option::<i32>::None),
            ]),
        );

        format_conds(
            &[cond1, cond2],
            &mut Postgres::parameter_factory(),
            &mut output,
        )
        .unwrap();

        assert_snapshot!(output, @r###" WHERE (("col1" = $1) AND ("col2" IS NULL) AND ("col3" IN ($2) OR "col3" IS NULL)) OR (("col1" != $3) AND ("col2" IS NOT NULL) AND ("col3" NOT IN ($4) AND "col3" IS NOT NULL))"###);
    }
}

mod select_query {
    use crate::manager::Value;

    use super::*;

    #[test]
    fn normal() {
        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2"],
            conds: vec1![cond],
            order_by: None,
            offset: None,
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1"###);
    }

    #[test]
    fn empty_cond() {
        let mut cond1 = Condition::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));

        let cond2 = Condition::<Postgres>::default();

        let query = SelectQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2"],
            conds: vec1![cond1, cond2],
            order_by: None,
            offset: None,
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table""###);
    }

    #[test]
    fn order_by() {
        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2"],
            conds: vec1![cond],
            order_by: Some((vec1!["col1", "col2"], Order::Asc)),
            offset: None,
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1 ORDER BY "col1", "col2" ASC"###);
    }

    #[test]
    fn skip() {
        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2"],
            conds: vec1![cond],
            order_by: None,
            offset: Some(3),
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1 SKIP 3"###);
    }

    #[test]
    fn take() {
        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2"],
            conds: vec1![cond],
            order_by: None,
            offset: None,
            limit: Some(3),
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1 TAKE 3"###);
    }

    #[test]
    fn combined() {
        let mut cond1 = Condition::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));
        cond1.add_col("col2", FindOperator::Eq(Box::new(Option::<i32>::None)));
        cond1.add_col(
            "col3",
            FindOperator::In(vec1![
                Box::new(2) as Box<dyn Value<Postgres>>,
                Box::new(Option::<i32>::None)
            ]),
        );

        let mut cond2 = Condition::<Postgres>::default();
        cond2.add_col("col1", FindOperator::Ne(Box::new(1)));
        cond2.add_col("col2", FindOperator::Ne(Box::new(Option::<i32>::None)));
        cond2.add_col(
            "col3",
            FindOperator::NotIn(vec1![
                Box::new(2) as Box<dyn Value<Postgres>>,
                Box::new(Option::<i32>::None)
            ]),
        );

        let query = SelectQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2"],
            conds: vec1![cond1, cond2],
            order_by: Some((vec1!["col1", "col2"], Order::Asc)),
            offset: Some(3),
            limit: Some(5),
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE (("col1" = $1) AND ("col2" IS NULL) AND ("col3" IN ($2) OR "col3" IS NULL)) OR (("col1" != $3) AND ("col2" IS NOT NULL) AND ("col3" NOT IN ($4) AND "col3" IS NOT NULL)) ORDER BY "col1", "col2" ASC SKIP 3 TAKE 5"###);
    }
}

mod count_query {
    use super::*;

    #[test]
    fn normal() {
        let mut cond = Condition::<Postgres>::default();
        cond.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = CountQuery {
            table_name: "table",
            conds: vec1![cond],
        };

        assert_snapshot!(query.to_string(), @r###"SELECT COUNT (*) AS "cnt" FROM "table" WHERE "col1" = $1"###);
    }

    #[test]
    fn empty_cond() {
        let mut cond1 = Condition::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));
        let cond2 = Condition::default();

        let query = CountQuery {
            table_name: "table",
            conds: vec1![cond1, cond2],
        };

        assert_snapshot!(query.to_string(), @r###"SELECT COUNT (*) AS "cnt" FROM "table""###);
    }
}

mod insert_query {
    use super::*;

    #[test]
    fn normal() {
        let mut values = Values::<Postgres>::default();
        values.add_col("col1", Box::new(1));

        let query = InsertQuery {
            table_name: "table",
            col_names: vec1!["col1"],
            values: vec1![values],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1") VALUES ($1)"###);
    }

    #[test]
    fn multiple_values() {
        let mut values1 = Values::<Postgres>::default();
        values1.add_col("col1", Box::new(1));
        values1.add_col("col2", Box::new(1));

        let mut values2 = Values::<Postgres>::default();
        values2.add_col("col1", Box::new(1));
        values2.add_col("col2", Box::new(1));

        let query = InsertQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2"],
            values: vec1![values1, values2],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1", "col2") VALUES ($1, $2), ($3, $4)"###);
    }

    #[test]
    fn values_with_different_column_sets() {
        let mut values1 = Values::<Postgres>::default();
        values1.add_col("col1", Box::new(1));
        values1.add_col("col2", Box::new(1));

        let mut values2 = Values::<Postgres>::default();
        values2.add_col("col2", Box::new(1));
        values2.add_col("col3", Box::new(1));

        let query = InsertQuery {
            table_name: "table",
            col_names: vec1!["col1", "col2", "col3"],
            values: vec1![values1, values2],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1", "col2", "col3") VALUES ($1, $2, DEFAULT), (DEFAULT, $3, $4)"###);
    }
}

mod insert_returning_query {
    use super::*;

    #[test]
    fn normal() {
        let mut values = Values::<Postgres>::default();
        values.add_col("col1", Box::new(1));

        let query = InsertReturningQuery {
            insert_query: InsertQuery {
                table_name: "table",
                col_names: vec1!["col1", "col2"],
                values: vec1![values],
            },
            returning_cols: vec1!["col1", "col2"],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1", "col2") VALUES ($1, DEFAULT) RETURNING "col1", "col2""###);
    }
}

mod update_query {
    use super::*;

    #[test]
    fn normal() {
        let mut conds = Condition::default();
        conds.add_col("col1", FindOperator::Eq(Box::new(1)));

        let mut values = Values::default();
        values.add_col("col2", Box::new(2));

        let query = UpdateQuery::<Postgres> {
            table_name: "table",
            conds: vec1![conds],
            new_values: values,
        };

        assert_snapshot!(query.to_string(), @r###"UPDATE "table" SET "col2" = $1 WHERE "col1" = $2"###);
    }

    #[test]
    fn empty_values() {
        let mut conds = Condition::default();
        conds.add_col("col1", FindOperator::Eq(Box::new(1)));

        let values = Values::default();

        let query = UpdateQuery::<Postgres> {
            table_name: "table",
            conds: vec1![conds],
            new_values: values,
        };

        assert_snapshot!(query.to_string(), @"<UPDATE query with no new values>");
    }
}

mod delete_query {
    use super::*;

    #[test]
    fn normal() {
        let mut conds = Condition::default();
        conds.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = DeleteQuery::<Postgres> {
            table_name: "table",
            conds: vec1![conds],
        };

        assert_snapshot!(query.to_string(), @r###"DELETE FROM "table" WHERE "col1" = $1"###);
    }
}
