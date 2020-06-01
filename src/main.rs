use std::env;
use serde:: {Serialize, Deserialize};
use serde_json::from_str;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use chrono::{Utc, DateTime, TimeZone};
use oracle::Connection as OracleConnection;
use mysql::{PooledConn, Pool, Params};
use mysql::prelude::Queryable;

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    nexus_oracle_url: String,
    nexus_oracle_username: String,
    nexus_oracle_password: String,
    rcontrol_oracle_url:String,
    rcontrol_oracle_username:String,
    rcontrol_oracle_password:String,
    mysql_url: String,
    merchant_sql: Vec<MerchantSql>,
    trade_sql: Vec<TradeSql>,
}

#[derive(Serialize, Deserialize, Debug)]
struct MerchantSql {
    query_sql: String,
    insert_sql: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct TradeSql {
    condition_sql: String,
    query_sql: String,
    insert_sql: String,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("------开始读取配置文件-------");

    if args.capacity() < 2 {
        panic!("缺少配置文件参数");
    }
    let config_name:String = args.get(1).cloned().unwrap();
    println!("配置文件名称：{:?}", config_name);

    let mut config_file: File = File::open(&config_name).unwrap_or_else(|error| {
        panic!("{}打开异常, {}", &config_name, error);
    });
    let mut config_content: String = String::new();
    config_file.read_to_string(& mut config_content).unwrap_or_else(|error| {
        panic!("{}读取异常, {}", &config_name, error);
    });
    if config_content.is_empty() {
        panic!("{:?}不能为空", &config_name);
    }

    let config: Config = match from_str(&config_content) {
        Ok(config) => config,
        Err(error) => panic!("{:?}不是一个合法json，{:?}", &config_file, error)
    };
    println!("配置文件详情：{:#?}", config);
    println!("-------配置文件读取完成---------");

    println!("-------开始读取脚本最后一次运行时间----------");
    let date_formatter: String = String::from("%Y-%m-%d");
    let now: DateTime<Utc> = Utc::now();
    let now_str:String = now.format(&date_formatter).to_string();
    println!("当前日期：{}", now_str);
    let yesterday: DateTime<Utc> = Utc.timestamp_millis(now.timestamp_millis() - 24 * 3600 * 1000);
    let yesterday_str: String = yesterday.format(&date_formatter).to_string();
    println!("昨日日期：{}", yesterday_str);
    let mut  file: File = OpenOptions::new().read(true).write(true).open("last_run.ini").unwrap_or_else(|error| {
        println!("读取文件last_run.ini异常, {}", error);
        println!("开始创建last_run.ini文件");
        let new_file: File = File::create("last_run.ini").unwrap_or_else( |err|{
            panic!("创建文件last_run.ini异常, {}", err);
        });
        println!("创建last_run_ini文件成功");
        return new_file;
    });

    let nexus_oracle_conn = oracle_conn(&config.nexus_oracle_url, &config.nexus_oracle_username, &config.nexus_oracle_password);
    let mut mysql_conn = mysql_conn(&config);

    // 同步商户信息
    for merchant in &config.merchant_sql {
        oracle_to_mysql(&nexus_oracle_conn,  &mut mysql_conn, &merchant.query_sql, &merchant.insert_sql);
    }

    // 同步交易信息
    for trade in &config.trade_sql {
        let new_query_sql_vec = condition_oracle_to_mysql(&nexus_oracle_conn,  &trade.condition_sql, &trade.query_sql);
        let rcontrol_oracle_conn = oracle_conn(&config.rcontrol_oracle_url, &config.rcontrol_oracle_username, &config.rcontrol_oracle_password);
        for query_sql in new_query_sql_vec {
            oracle_to_mysql(&rcontrol_oracle_conn, &mut mysql_conn, &query_sql, &trade.insert_sql);
        }
    }

    // 只能覆盖写入的字符长度
    file.write(now_str.as_bytes()).unwrap_or_else(|e| {
        panic!("写入最后运行日期异常, {}", e);
    });

    println!("-------完成读取脚本最后一次运行时间----------");

}

// 获取oracle连接
fn oracle_conn(url: &String, username: &String, password: &String) -> OracleConnection {
    let conn: OracleConnection = OracleConnection::connect(username, password, url)
        .unwrap_or_else(|err| {
            panic!("建立oracle连接失败，连接信息：username:{}, password:{}, url:{}. 错误信息:{}", username, password, url, err);
        });
    return conn;
}

// 获取mysql连接
fn mysql_conn(config: &Config) -> PooledConn {
    let pool = Pool::new(&config.mysql_url).unwrap_or_else(|err| {
        panic!("创建mysql连接失败，连接信息：{}, 错误信息:{}", &config.mysql_url, err);
    });
    let conn = pool.get_conn().unwrap_or_else(|err| {
        panic!("从mysql连接池获取连接异常，{}", err);
    });
    return conn;
}

fn oracle_to_mysql(oracle_conn: &OracleConnection, mysql_conn: &mut PooledConn, query_sql: &String, insert_sql: &String) {
    let rows = oracle_conn.query(query_sql, &[]).unwrap_or_else(|err| {
        panic!("查询sql:{}异常，{}", query_sql, err);
    });

    let mut vec: Vec<Params> = Vec::new();
    let len:usize = rows.column_info().len();
    for row_result in rows {
        let mut param = Vec::new();
        let row = row_result.unwrap();
        for number in 0..len {
            let value: String = row.get(number).unwrap_or(String::from(""));
            param.push(value);
        }
        let params:Params = Params::from(param);
        vec.push(params);
    }

    if vec.len() > 0 {
        mysql_conn.exec_batch(insert_sql, vec).unwrap_or_else(|err| {
            panic!("写入mysql系统异常，{}", err);
        });
    }
}

fn condition_oracle_to_mysql(oracle_conn: &OracleConnection,
                             condition_sql: &String,
                             query_sql:&String) -> Vec<String> {
    let merchant_rows = oracle_conn.query(condition_sql, &[]).unwrap_or_else(|err| {
        panic!("查询nexus系统商户异常, sql:{}, 异常信息:{}", condition_sql, err);
    });

    let mut  new_query_sql_vec = Vec::new();
    let mut  merchants: Vec<String> = Vec::new();
    for row_result in merchant_rows {
        let row = row_result.unwrap();
        let merchant:String = row.get(0).unwrap_or(String::from("_"));
        merchants.push(merchant);
        if merchants.len() == 500 {
            let temp_merchants = merchants.clone();
            merchants.clear();
            let new_query_sql = query_sql.replace(&String::from("merchants"), &temp_merchants.join(&String::from("','")));
            println!("交易查询sql:{}", &new_query_sql);
            new_query_sql_vec.push(new_query_sql);
        }
    }
    if merchants.len() > 0 {
        let new_query_sql = query_sql.replace(&String::from("merchants"), &merchants.join(&String::from("','")));
        println!("交易查询sql:{}", new_query_sql);
        new_query_sql_vec.push(new_query_sql);
    }
    return new_query_sql_vec;
}