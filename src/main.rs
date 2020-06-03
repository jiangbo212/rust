use std::env;
use std::fs;
use serde:: {Serialize, Deserialize};
use serde_json::from_str;
use std::fs::{File, OpenOptions};
use std::io::{Read};
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
    let merchant_last_file_name:String = String::from("merchant_last_run.ini");
    let trade_last_file_name:String = String::from("trade_last_run.ini");

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


    let nexus_oracle_conn = oracle_conn(&config.nexus_oracle_url, &config.nexus_oracle_username, &config.nexus_oracle_password);
    let mut mysql_conn = mysql_conn(&config);

    let date_formatter: String = String::from("%Y-%m-%d %H:%M:%S");
    let now: DateTime<Utc> = Utc::now();
    let now_str:String = now.format(&date_formatter).to_string();
    println!("当前日期：{}", now_str);

    println!("---------开始同步商户信息----------");
    // 获取最近一次商户同步时间
    let merchant_last_run_time: String = get_last_run_time(&merchant_last_file_name);
    println!("最近一次商户同步时间:{}", &merchant_last_run_time);
    // 同步商户信息
    for merchant in &config.merchant_sql {
        let new_query_sql =&merchant.query_sql.replace(&String::from("last_update_time"), &merchant_last_run_time);
        oracle_to_mysql(&nexus_oracle_conn, &mut mysql_conn, &new_query_sql, &merchant.insert_sql, &Vec::new());
    }

    fs::write(&merchant_last_file_name, &now_str).unwrap();
    println!("---------同步商户信息结束----------");

    println!("---------开始同步交易信息----------");
    // 获取最后一次交易同步时间
    let trade_last_run_time:String = get_last_run_time(&trade_last_file_name);
    println!("最近一次交易同步时间:{}", &trade_last_run_time);
    let mut runtime: DateTime<Utc> = Utc.datetime_from_str(&trade_last_run_time, &date_formatter).unwrap();
    while now.timestamp_millis() > runtime.timestamp_millis() {
        let start_time_str: String = runtime.format(&date_formatter).to_string();
        runtime = Utc.timestamp_millis(runtime.timestamp_millis() + 24 * 3600 * 1000);
        let mut end_time_str:String = runtime.format(&date_formatter).to_string();
        if now.timestamp_millis() < runtime.timestamp_millis() {
            end_time_str = now.format(&date_formatter).to_string();
        }
        println!("交易同步开始, {} ~ {}", start_time_str, end_time_str);
        // 同步交易信息
        let mut rcontrol_oracle_conn = oracle_conn(&config.rcontrol_oracle_url, &config.rcontrol_oracle_username, &config.rcontrol_oracle_password);
        for trade in &config.trade_sql {
            let merchants = merchants_info(&nexus_oracle_conn,  &trade.condition_sql);
            for mut merchant in merchants {
                let merchant_code = merchant.get(0).unwrap();
                let new_query_sql = &trade.query_sql
                    .replace(&String::from("mer_code"), &merchant_code)
                    .replace(&String::from("st_time"), &start_time_str)
                    .replace(&String::from("ed_time"), &end_time_str);
                merchant.remove(0);
                oracle_to_mysql(&mut rcontrol_oracle_conn, &mut mysql_conn, &new_query_sql, &trade.insert_sql, &merchant);
            }
        }
    }
    fs::write(&trade_last_file_name, &now_str).unwrap();
    println!("---------同步交易信息结束----------");

    println!("-------完成读取脚本最后一次运行时间----------");
}

fn get_last_run_time(last_run_file_name: &String) -> String {
    let mut merchant_last_file: File = OpenOptions::new().read(true).write(true).create(true).open(last_run_file_name).unwrap();
    let mut last_update_time:String = String::new();
    merchant_last_file.read_to_string(&mut last_update_time).unwrap_or_else(|err| {
        panic!("读文件异常,文件名:{},异常信息:{}", last_run_file_name, err);
    });
    return last_update_time;
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

fn oracle_to_mysql(oracle_conn: &OracleConnection,
                   mysql_conn: &mut PooledConn,
                   query_sql: &String,
                   insert_sql: &String,
                   insert_condition: &Vec<String>) {
    // println!("query_sql:{}", query_sql);
    let rows = oracle_conn.query(query_sql, &[]).unwrap_or_else(|err| {
        panic!("查询sql:{}异常，{}", query_sql, err);
    });

    let mut vec: Vec<Params> = Vec::new();
    let len:usize = rows.column_info().len();
    for row_result in rows {
        let mut param = insert_condition.clone();
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

fn merchants_info(oracle_conn: &OracleConnection, condition_sql: &String) -> Vec<Vec<String>> {
    let merchant_rows = oracle_conn.query(condition_sql, &[]).unwrap_or_else(|err| {
        panic!("查询商户异常, sql:{}, 异常信息:{}", condition_sql, err);
    });

    let mut  merchants: Vec<Vec<String>> = Vec::new();
    let len = merchant_rows.column_info().len();
    for row_result in merchant_rows {
        let mut merchant = Vec::new();
        let row = row_result.unwrap();
        for  number in 0..len {
            let mut value: String = row.get(number).unwrap_or(String::from(""));
            // 商户号默认_
            if number == 0 {
                value = row.get(number).unwrap_or(String::from("_"));
            }
            merchant.push(value);
        }
        merchants.push(merchant);
    }
    return merchants;
}