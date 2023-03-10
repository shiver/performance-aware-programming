use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt};
use std::{
    fs::File,
    io::{Cursor, Read, Seek},
};

fn load_binary(path: &str) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    Ok(buffer)
}

#[derive(Debug, PartialEq, Clone, Copy)]
#[allow(dead_code)]
enum ModField {
    MemoryNoDisplacement = 0b00,
    MemoryDisplacement8bit = 0b01,
    MemoryDisplacement16bit = 0b10,
    RegisterNoDisplacement = 0b11,
}

#[repr(u8)]
#[derive(Debug)]
#[allow(non_camel_case_types, dead_code)]
enum Registers {
    al = 0b0000,
    cl = 0b0001,
    dl = 0b0010,
    bl = 0b0011,
    ah = 0b0100,
    ch = 0b0101,
    dh = 0b0110,
    bh = 0b0111,
    ax = 0b1000,
    cx = 0b1001,
    dx = 0b1010,
    bx = 0b1011,
    sp = 0b1100,
    bp = 0b1101,
    si = 0b1110,
    di = 0b1111,
}

const SHOW_COMMENTS: bool = false;

fn get_displacement(mode: ModField, rm: u8, value: i16) -> String {
    match mode {
        ModField::MemoryNoDisplacement => match rm {
            0b000 => format!("[bx + si]"),
            0b001 => format!("[bx + di]"),
            0b010 => format!("[bp + si]"),
            0b011 => format!("[bp + di]"),
            0b100 => format!("[si]"),
            0b101 => format!("[di]"),
            0b110 if value == 0 => format!("[bp]"),
            0b110 => format!("[{}]", value),
            0b111 => format!("[bx]"),

            _ => unreachable!(),
        },

        ModField::MemoryDisplacement16bit | ModField::MemoryDisplacement8bit => {
            let value_str = if value >= 0 {
                format!("+ {}", value)
            } else {
                format!("- {}", value.abs())
            };

            match rm {
                0b000 => format!("[bx + si {}]", value_str),
                0b001 => format!("[bx + di {}]", value_str),
                0b010 => format!("[bp + si {}]", value_str),
                0b011 => format!("[bp + di {}]", value_str),
                0b100 => format!("[si {}]", value_str),
                0b101 => format!("[di {}]", value_str),
                0b110 if value == 0 => format!("[bp]"),
                0b110 => format!("[{}]", value),
                0b111 => format!("[bx {}]", value_str),

                _ => unreachable!(),
            }
        }

        _ => unreachable!(),
    }
}

fn main() -> Result<()> {
    let binary_filename = std::env::args()
        .nth(1)
        .expect("Please provide an assembled filename");
    let binary = load_binary(&binary_filename)?;

    let mut cursor = Cursor::new(binary);

    println!("bits 16");

    'main: loop {
        println!("");

        let opcode = match cursor.read_u8() {
            Ok(byte) => byte,
            Err(_) => break,
        };

        if (opcode >> 2) == 0b100010 {
            // MOV - Register/memory to/from register

            let w_field = opcode & 1; // how wide is the data, 8 or 16 bits
            let d_field = opcode >> 1 & 1; // if 1, destination is a register

            let next_byte = cursor.read_u8()?;

            let rm = next_byte & 0b111;
            let mode = unsafe { std::mem::transmute::<u8, ModField>(next_byte >> 6) };
            let reg = unsafe {
                std::mem::transmute::<u8, Registers>(w_field << 3 | (next_byte >> 3 & 0b111))
            };

            print!("mov ");

            let value: i16 = match mode {
                ModField::MemoryDisplacement8bit => cursor.read_i8()? as i16,
                ModField::MemoryDisplacement16bit => cursor.read_i16::<LittleEndian>()?,
                ModField::MemoryNoDisplacement if rm == 0b110 => {
                    cursor.read_i16::<LittleEndian>()?
                }
                _ => 0,
            };

            let (source, destination) = match mode {
                ModField::MemoryNoDisplacement => {
                    let displacement = get_displacement(mode, rm, value);

                    match d_field {
                        0 => (displacement, format!("{:?}", reg)),
                        1 => (format!("{:?}", reg), displacement),
                        _ => unreachable!(),
                    }
                }

                ModField::MemoryDisplacement16bit | ModField::MemoryDisplacement8bit => {
                    let displacement = get_displacement(mode, rm, value);

                    match d_field {
                        0 => (displacement, format!("{:?}", reg)),
                        1 => (format!("{:?}", reg), displacement),
                        _ => unreachable!(),
                    }
                }

                ModField::RegisterNoDisplacement => {
                    let other_reg =
                        unsafe { std::mem::transmute::<u8, Registers>(w_field << 3 | rm) };

                    match d_field {
                        0 => (format!("{:?}", other_reg), format!("{:?}", reg)),
                        1 => (format!("{:?}", reg), format!("{:?}", other_reg)),
                        _ => unreachable!(),
                    }
                }
            };

            print!("{}, {}", source, destination);

            if SHOW_COMMENTS {
                print!(
                    " ; Register/memory to/from register \n\topcode={:#b} next_byte={:#b}\n\td={} w={} mod={:#03b} reg={:?} rm={:#b}\n",
                    opcode, next_byte, d_field, w_field, mode as u8, reg, rm
                );
            }
        } else if opcode >> 1 == 0b1100011 {
            // MOV - Immediate to register/memory
            print!("mov ");

            let w_field = opcode & 0b1;
            let next_byte = cursor.read_u8()?;
            let rm = next_byte & 0b111;
            let mod_field = next_byte >> 6;
            let mode = unsafe { std::mem::transmute::<u8, ModField>(mod_field) };

            let displacement_value = match w_field {
                0 => cursor.read_i8()? as i16,
                1 => cursor.read_i16::<LittleEndian>()?,
                _ => unreachable!(),
            };

            let (destination, source) = match mode {
                ModField::RegisterNoDisplacement => {
                    let reg = unsafe { std::mem::transmute::<u8, Registers>(rm) };
                    let value_str = match w_field {
                        0 => format!("byte {}", displacement_value),
                        1 => format!("word {}", displacement_value),
                        _ => unreachable!(),
                    };

                    (format!("{:?}", reg), value_str)
                },

                ModField::MemoryNoDisplacement => {
                    let displacement = get_displacement(mode, rm, displacement_value);

                    let value_str = match w_field {
                        0 => format!("byte {}", displacement_value),
                        1 => format!("word {}", displacement_value),
                        _ => unreachable!(),
                    };

                    (format!("{}", displacement), value_str)
                },

                _ => {
                    let displacement = get_displacement(mode, rm, displacement_value);

                    let data: i16 = if w_field == 0 {
                        cursor.read_i8()? as i16
                    } else {
                        cursor.read_i16::<LittleEndian>()?
                    };

                    let value_str = match w_field {
                        0 => format!("byte {}", data),
                        1 => format!("word {}", data),
                        _ => unreachable!(),
                    };

                    (format!("{}", displacement), value_str)
                }
            };

            print!("{}, {}", destination, source);

            if SHOW_COMMENTS {
                print!(
                    "; Immediate to register/memory \n\topcode={:#b} w={} disp={} mod={:#b}, rm={:#b}\n",
                    opcode, w_field, displacement_value, mod_field, rm
                );
            }
        } else if opcode >> 4 == 0b1011 {
            // MOV - Immediate to register
            let w_field = opcode >> 3 & 0b1;
            let reg =
                unsafe { std::mem::transmute::<u8, Registers>(w_field << 3 | (opcode & 0b111)) };

            print!("mov {:?}, ", reg);
            if w_field == 0 {
                print!("{}", cursor.read_i8()?);
            } else {
                print!("{}", cursor.read_i16::<LittleEndian>()?);
            }
        } else if opcode >> 1 == 0b1010000 {
            // MOV - Memory to accumulator
            println!("mov ;Memory to accumulator");
        } else if opcode >> 1 == 0b1010001 {
            // MOV - Accumulator to memory
            println!("mov ;Accumulator to memory");
        } else if opcode == 0b10001110 {
            // MOV - Register/memory to segment register
            println!("mov ;Register/memory to segment register");
        } else if opcode == 0b10001100 {
            // MOV - Segment register to register/memory
            println!("mov ;Segment register to register/memory");
        }
    }

    Ok(())
}
