#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
extern crate rocket_multipart_form_data;
extern crate serde;

use rocket::State;
use rocket::http::ContentType;
use rocket::Data;
use rocket_contrib::json::Json;
use rocket_multipart_form_data::{
    MultipartFormData, MultipartFormDataField, MultipartFormDataOptions,
};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
struct Rom {
    id: String,
    #[serde(rename = "programState")] program_state: String
}

#[derive(Deserialize, Serialize)]
struct CpuFlags {
    sign: bool,
    zero: bool,
    #[serde(rename = "auxCarry")]
    aux_carry: bool,
    parity: bool,
    carry: bool,
}

#[derive(Deserialize, Serialize)]
struct CpuState {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
    #[serde(rename = "stackPointer")]
    stack_pointer: u16,
    #[serde(rename = "programCounter")]
    program_counter: u16,
    cycles: u64,
    flags: CpuFlags,
    #[serde(rename = "interruptsEnabled")]
    interrupts_enabled: bool,
}

#[derive(Deserialize, Serialize)]
struct Cpu {
    state: CpuState,
    id: String,
    opcode: u8,
}

struct Config {
    read_range_api: String,
    initialise_data_api: String,
    start_program_api: String,
}

#[post("/initialise", format = "json", data = "<cpu>")]
fn initialise(mut cpu: Json<Cpu>) -> Option<Json<Cpu>> {
    cpu.state.program_counter = 0x100;
    return Some(cpu);
}

#[post("/interruptCheck", format = "json", data = "<cpu>")]
fn interrupt_check(cpu: Json<Cpu>) -> Option<Json<Cpu>> {
    return Some(cpu);
}

#[post("/in?<operand1>", format = "json", data = "<cpu>")]
fn in_api(operand1: Option<u8>, mut cpu: Json<Cpu>) -> Option<Json<Cpu>> {
    let _port = operand1;
    cpu.state.a = 0x0;
    Some(cpu)
}

#[post("/out?<operand1>", format = "json", data = "<cpu>")]
fn out_api(operand1: Option<u8>, mut cpu: Json<Cpu>, config: State<Config>) -> Option<Json<Cpu>> {
    let port = operand1.unwrap();

    if port == 0 {
        // Only accept OUT on port 0 as this is a hack to handle BIOS functions
        let operation = cpu.state.c;

        if operation == 0x0 {
            // P_TERMCPM - Exit application
            panic!("Application exit due to BIOS call with C = 0");
        } else if operation == 0x1 {
            // C_READ - Unimplemented
            cpu.state.a = 0;
        } else if operation == 0x2 {
            // C_WRITE - Write single ascii character
            println!("{}", std::str::from_utf8(&vec![cpu.state.e]).unwrap());
        } else if operation == 0x9 {
            // C_WRITESTR - Write string
            let address = ((cpu.state.d as u16) << 8) | cpu.state.e as u16;
            let mut constructed_string = Vec::<u8>::new();
            println!("\nC_WRITESTR {}", address);
            let client = reqwest::blocking::Client::new();
            let str_response = client
                .get(format!("{}?id={}&address={}&length={}", config.read_range_api, cpu.id, address, 0xFFFF - address))
                .send()
                .unwrap();
            if str_response.status() != reqwest::StatusCode::OK {
                panic!("Invalid response from read memory api {}: {}", str_response.status(), str_response.text().unwrap());
            }
            
            for b in base64::decode(str_response.text().unwrap()).unwrap() {
                if b == 36 { break; }
                constructed_string.push(b);
            }
            
            println!("{}", std::str::from_utf8(&constructed_string).unwrap());
        } else {
            panic!("Unknown bios function called");
        }
    }

    Some(cpu)
}

#[get("/status")]
fn status() -> &'static str {
    "Healthy"
}

#[post("/load", data = "<rom>")]
fn load_data(content_type: &ContentType, rom: Data, config: State<Config>) -> String {
    let options = MultipartFormDataOptions::with_multipart_form_data_fields(vec![
        MultipartFormDataField::file("rom").size_limit(100000),
    ]);

    let multipart_form_data = MultipartFormData::parse(content_type, rom, options).unwrap();
    let rom_data = &multipart_form_data.files.get("rom").unwrap()[0];
    let mut file = File::open(&rom_data.path).unwrap();
    let mut rom_data: Vec<u8> = vec![];
    file.read_to_end(&mut rom_data).unwrap();
    rom_data.resize(0x10000 - 0x100, 0x0); // Must have exactly 0xFFFF bytes in address space but need the first 0xFF for bios

    let computer_id = Uuid::new_v4();

    let mut program_data = vec![0x76; 0x100]; // Fill BIOS with HLT instruction
    program_data[5] = 0xD3; // Set bios operation at 0x05 as OUT 0 to use test harness
    program_data[6] = 0x00;
    program_data[7] = 0x00;
    program_data[8] = 0xC9;
    program_data.extend(rom_data);

    let rom = Rom {
        id: computer_id.to_string(),
        program_state: base64::encode(program_data)
    };

    // Initialise the address space with the constructed program data
    let client = reqwest::blocking::Client::new();
    let init_response = client
        .post(format!("{}", config.initialise_data_api))
        .json(&rom)
        .send()
        .unwrap();
    if init_response.status() != reqwest::StatusCode::OK {
        panic!("Could not initialise memory");
    }

    // Fire and forget the program start
    let start_response = client
        .post(format!("{}?id={}", config.start_program_api, rom.id))
        .body("")
        .send()
        .unwrap();
    if start_response.status() != reqwest::StatusCode::OK {
        panic!("Could not start program");
    }

    computer_id.to_hyphenated().to_string()
}

fn rocket() -> rocket::Rocket {
    let config = Config {
        read_range_api: match std::env::var("READ_RANGE_API") {
            Ok(url) => url,
            Err(_) => panic!("Couldn't read READ_RANGE_API environment variable"),
        },
        initialise_data_api: match std::env::var("INITIALISE_DATA_API") {
            Ok(url) => url,
            Err(_) => panic!("Couldn't read INITIALISE_DATA_API environment variable"),
        },
        start_program_api: match std::env::var("START_PROGRAM_API") {
            Ok(url) => url,
            Err(_) => panic!("Couldn't read START_PROGRAM_API environment variable"),
        }
    };

    rocket::ignite()
        .mount("/", routes![status])
        .mount(
            "/api/v1",
            routes![
                in_api,
                out_api,
                status,
                load_data,
                initialise,
                interrupt_check
            ],
        )
        .manage(config)
}

fn main() {
    rocket().launch();
}
