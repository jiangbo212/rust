use std::env;
use serde:: {Serialize, Deserialize};
use serde_json::from_str;
use std::fs::{File};
use std::io::{Read, Write};
use chrono::{Utc, DateTime, TimeZone};

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    oracle_url: String,
    oracle_username: String,
    oracle_password: i32,
    mysql_url: bool,
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
    let mut  file: File = File::open("last_run.ini").unwrap_or_else(|error| {
        println!("读取文件last_run.ini异常, {}", error);
        println!("开始创建last_run.ini文件");
        let new_file: File = File::create("last_run.ini").unwrap_or_else( |err|{
            panic!("创建文件last_run.ini异常, {}", err);
        });
        println!("创建last_run_ini文件成功");
        return new_file;
    });

    file.write_all(b"123").unwrap_or_else(|e| {
        panic!("写入最后运行日期异常, {}", e);
    });

    file.sync_all().unwrap_or_else(|error| {
        panic!("写入最后一次运行时间异常, {}", error);
    });

    println!("-------完成读取脚本最后一次运行时间----------");

}