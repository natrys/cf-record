use std::env;
use std::fmt::Write as fmtWrite;
use std::io::Write;

use anyhow::Result;
use clap::Clap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tabwriter::TabWriter;

lazy_static! {
    static ref ZONE: String = env::var("CF_ZONE_ID").expect("Define Zone ID in $CF_ZONE_ID");
    static ref TOKEN: String =
        env::var("CF_ZONE_TOKEN").expect("Define Zone Token in $CF_ZONE_TOKEN");
    static ref ENDPOINT: String = format!(
        "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
        *ZONE
    );
}

fn record_endpoint(id: &str) -> String {
    format!("{}/{}", *ENDPOINT, id)
}

#[derive(Clap)]
#[clap(version = env!("CARGO_PKG_VERSION"), author = "Imran Khan")]
struct Config {
    #[clap(subcommand)]
    subcmd: Subcommand,
}

#[derive(Clap)]
enum Subcommand {
    #[clap(about = "Delete zone record")]
    Del(DelOpts),
    #[clap(about = "Set zone record")]
    Set(SetOpts),
    #[clap(about = "Show all zone records")]
    Show(ShowOpts),
}

#[derive(Clap)]
struct ShowOpts {
    #[clap(
        short = "f",
        default_value = "all",
        about = "Filter records by DNS type (e.g. A, CNAME etc.)"
    )]
    filter: String,
}

#[derive(Clap)]
struct SetOpts {
    #[clap(about = "Name of the record to set")]
    name: String,
    #[clap(
        default_value = "this_machine_ip",
        about = "Destination of the record to set"
    )]
    dest: String,
    #[clap(default_value = "A", about = "DNS type of the record to set")]
    r#type: String,
}

#[derive(Clap)]
struct DelOpts {
    #[clap(about = "Name of the record to delete")]
    name: String,
}

#[derive(Deserialize)]
struct Response {
    result: Vec<Entry>,
}

#[derive(Deserialize, Serialize, Clone)]
struct Entry {
    id: String,
    name: String,
    r#type: String,
    content: String,
}

fn show_rec(records: &Vec<Entry>, filter: &str) -> Result<()> {
    let stdout = std::io::stdout();
    let mut tw = TabWriter::new(stdout.lock());
    let mut line = String::new();
    for entry in records {
        if filter != "all" && entry.r#type != filter {
            continue;
        };
        writeln!(
            &mut line,
            "{}\t{}\t{}\t{}",
            entry.r#type, entry.name, entry.content, entry.id
        )?;
        tw.write_all(&line.as_bytes())?;
        line.clear();
    }
    tw.flush()?;

    Ok(())
}

fn del_rec(records: &Vec<Entry>, name: &str) -> Result<()> {
    match find_rec(records, name) {
        Some(entry) => {
            let resp = ureq::delete(&record_endpoint(&entry.id))
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {}", *TOKEN))
                .call();
            if resp.ok() {
                println!("Successfully deleted {}", entry.name);
            } else {
                println!("Error deleting {}", entry.name);
            }
        }
        _ => println!("No such record exists"),
    }

    Ok(())
}

fn set_rec(records: &Vec<Entry>, name: &str, dest: &str, r#type: &str) -> Result<()> {
    let destination = match dest {
        "this_machine_ip" => {
            let resp = ureq::get("https://ipinfo.io/ip").call().into_string()?;
            resp.trim().to_owned()
        }
        _ => dest.to_owned(),
    };

    match find_rec(records, name) {
        Some(entry) => {
            println!("{} already exists, trying to update...", name);
            let new = Entry {
                id: entry.id.clone(),
                content: destination,
                name: entry.name.clone(),
                r#type: entry.r#type.clone(),
            };
            let resp = ureq::put(&record_endpoint(&entry.id))
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {}", *TOKEN))
                .send_json(serde_json::from_str(&serde_json::to_string(&new)?)?);
            if resp.ok() {
                println!(
                    "Successfully Updated {} with {} (type: {})",
                    name, new.content, new.r#type
                );
            } else {
                println!("Error updating {}", name);
            }
        }

        _ => {
            println!("No such record exists, trying to create new...");
            let new = Entry {
                id: "".to_owned(),
                content: destination,
                name: name.to_owned(),
                r#type: r#type.to_owned(),
            };
            let resp = ureq::post(&*ENDPOINT)
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {}", *TOKEN))
                .send_json(serde_json::from_str(&serde_json::to_string(&new)?)?);
            if resp.ok() {
                println!("Successfully Updated {} to point to {}", name, new.content);
            } else {
                println!("Error updating {}", name);
            }
        }
    }

    Ok(())
}

fn find_rec<'a>(records: &'a Vec<Entry>, name: &str) -> Option<&'a Entry> {
    records.iter().find(|&entry| entry.name == name)
}

fn main() -> Result<()> {
    let conf: Config = Config::parse();

    let resp: Response = serde_json::from_str(
        &ureq::get(&ENDPOINT)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", *TOKEN))
            .call()
            .into_string()?,
    )?;

    let records = resp.result;

    match conf.subcmd {
        Subcommand::Show(s) => show_rec(&records, &s.filter),
        Subcommand::Set(s) => set_rec(&records, &s.name, &s.dest, &s.r#type),
        Subcommand::Del(s) => del_rec(&records, &s.name),
    }
}
