#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
extern crate rocket_multipart_form_data;
extern crate serde;

use postgres::Client;
use postgres::NoTls;
use rocket::http::ContentType;
use rocket::Data;
use rocket_contrib::json::Json;
use rocket_multipart_form_data::{
    MultipartFormData, MultipartFormDataField, MultipartFormDataOptions,
};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::fs::File;
use std::io::Read;
use uuid::Uuid;

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
fn out_api(operand1: Option<u8>, mut cpu: Json<Cpu>) -> Option<Json<Cpu>> {
    let port = operand1.unwrap();
    println!("OUT called with port {} and c = {}", port, cpu.state.c);

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
            let connection_string = match std::env::var("Database__ConnectionString") {
                Ok(url) => url,
                Err(_) => panic!("Couldn't read Database__ConnectionString environment variable"),
            };
            let address = ((cpu.state.d as u16) << 8) | cpu.state.e as u16;
            println!("C_WRITESTR {}", address);
            let mut client = Client::connect(&connection_string, NoTls).unwrap();
            let rows = client.query(
                "SELECT value FROM public.address_space WHERE computer_id=$1 AND address >= $2 AND address < (SELECT address FROM public.address_space WHERE computer_id = $1 AND value = 36 AND address > $2 LIMIT 1)", 
                &[&Uuid::parse_str(&cpu.id).unwrap(), &(address as i32)]).unwrap();
            let ascii_values = rows
                .iter()
                .map(|row| row.get::<usize, i16>(0))
                .map(|i| i.try_into().unwrap())
                .collect::<Vec<u8>>();

            println!("{}", std::str::from_utf8(&ascii_values).unwrap());
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
fn load_data(content_type: &ContentType, rom: Data) -> String {
    let options = MultipartFormDataOptions::with_multipart_form_data_fields(vec![
        MultipartFormDataField::file("rom").size_limit(1024),
    ]);

    let multipart_form_data = MultipartFormData::parse(content_type, rom, options).unwrap();
    let rom_data = &multipart_form_data.files.get("rom").unwrap()[0];
    let mut file = File::open(&rom_data.path).unwrap();
    let mut rom_data: Vec<u8> = vec![];
    file.read_to_end(&mut rom_data).unwrap();
    rom_data.resize(0x10000 - 0x100, 0x0); // Can have at most 0xFFFF bytes in address space but need the first 0xFF for bios
    print!("{:?}", rom_data);

    let connection_string = match std::env::var("Database__ConnectionString") {
        Ok(url) => url,
        Err(_) => panic!("Couldn't read Database__ConnectionString environment variable"),
    };
    let mut client = Client::connect(&connection_string, NoTls).unwrap();
    let mut tran = client.build_transaction().start().unwrap();
    let row = tran
        .query_one(
            "INSERT INTO public.computer (id, state) VALUES ($1, '{}') RETURNING id",
            &[&Uuid::new_v4()],
        )
        .unwrap();
    let computer_id: Uuid = row.get(0);

    let mut bios = vec![0x76; 0x100]; // Fill BIOS with HLT instruction
    bios[5] = 0xD3; // Set bios operation at 0x05 as OUT 0 to use test harness
    bios[6] = 0x00;
    bios[7] = 0x00;
    bios[8] = 0xC9;

    for (ix, byte) in bios.iter().chain(rom_data.iter()).enumerate() {
        tran.execute(
            "INSERT INTO public.address_space (computer_id, address, value) VALUES ($1, $2, $3)",
            &[&computer_id, &(ix as i32), &(*byte as i16)],
        )
        .unwrap();
    }

    tran.commit().unwrap();

    computer_id.to_hyphenated().to_string()
}

fn main() {
    rocket::ignite()
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
        .launch();
}
