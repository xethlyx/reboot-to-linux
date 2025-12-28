#!/usr/bin/env -S cargo +nightly -Zscript
#![windows_subsystem = "windows"]
use std::{collections::HashMap, process::Command};

use winsafe::{gui, prelude::{GuiEventsButton, GuiWindow}};

#[derive(Debug)]
struct BCDTable {
    // pub name: String,
    pub values: HashMap<String, String>
}

struct BCDTableParser<'a> {
    text: &'a [u8],
    index: usize
}

impl BCDTableParser<'_> {
    pub fn new(input: &'_ str) -> BCDTableParser<'_> {
        BCDTableParser { text: input.as_bytes(), index: 0 }
    }

    fn at(&self) -> Option<u8> {
        self.text.get(self.index).copied()
    }

    fn eat(&mut self) {
        if self.index >= self.text.len() {
            panic!("attempt to eat out of bounds")
        }

        self.index += 1;
    }

    fn expect(&mut self, value: u8) -> Result<(), String> {
        match self.at() {
            Some(current) if current == value => {
                self.eat();
                Ok(())
            }
            current => {
                Err(format!("expected {:?}, got {:?}", Some(value).map(|value| value as char), current.map(|value| value as char)))
            }
        }
    }

    pub fn parse(&mut self) -> Result<Vec<BCDTable>, String> {
        let mut output = vec![];

        while let Some(current) = self.at() {
            if current == b'\r' || current == b'\n' {
                self.eat();
                continue;
            }

            self.header()?;
            output.push(BCDTable {
                // name: self.header(),
                values: self.contents()?,
            });
        }

        Ok(output)
    }

    fn header(&mut self) -> Result<String, String> {
        let mut output = vec![];

        while let Some(current) = self.at() {
            if current == b'\r' {
                break;
            }

            output.push(current);
            self.eat();
        }

        self.expect(b'\r')?;
        self.expect(b'\n')?;
        while let Some(current) = self.at() {
            if current != b'-' {
                break;
            }
            self.eat();
        }
        self.expect(b'\r')?;
        self.expect(b'\n')?;

        String::from_utf8(output).map_err(|err| err.to_string())
    }

    fn contents(&mut self) -> Result<HashMap<String, String>, String> {
        let mut output = HashMap::new();

        while let Some(current) = self.at() {
            if current == b'\r' {
                break
            }
            let (key, value) = self.content_entry()?;
            output.insert(key, value);
        }

        Ok(output)
    }

    fn content_entry(&mut self) -> Result<(String, String), String> {
        let mut key = vec![];
        let mut value = vec![];

        while let Some(current) = self.at() {
            if current == b' ' {
                break;
            }

            key.push(current);
            self.eat();
        }

        self.expect(b' ')?;
        while let Some(current) = self.at() {
            if current != b' ' {
                break;
            }
            self.eat();
        }

        while let Some(current) = self.at() {
            if current == b'\r' {
                break;
            }

            value.push(current);
            self.eat();
        }

        self.expect(b'\r')?;
        self.expect(b'\n')?;

        Ok((String::from_utf8(key).map_err(|err| err.to_string())?, String::from_utf8(value).map_err(|err| err.to_string())?))
    }
}

fn main() {
    if let Err(err) = main_window() {
        gui::WindowMain::new(gui::WindowMainOpts::default())
            .hwnd()
            .MessageBox(&err, "Application Error", winsafe::co::MB::ICONERROR)
            .unwrap();
    }
}

fn main_window() -> Result<(), String> {
    let output = Command::new("bcdedit")
        .args(&["/enum", "firmware"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => {
            String::from_utf8(o.stdout).map_err(|err| err.to_string())?
        }
        Ok(o) => {
            let err = String::from_utf8(o.stdout).map_err(|err| err.to_string())?;
            return Err(format!("bcdedit returned non-zero status: {}\n{}", o.status, err));
        }
        Err(e) => {
            return Err(format!("failed to spawn bcdedit: {}", e));
        }
    };

    let output: Vec<_> = BCDTableParser::new(&output).parse()?.into_iter().filter(|val| val.values.contains_key("description") && val.values.contains_key("identifier")).collect();

    let wnd: &'static _ = Box::leak(Box::new(gui::WindowMain::new(
        gui::WindowMainOpts {
            title: "Reboot to Linux",
            size: gui::dpi(300, 12 + (32 * output.len() as i32) - 8 + 12),
            ..Default::default()
        }
    )));

    for (i, bcd) in output.iter().enumerate() {
        let btn = gui::Button::new(wnd, gui::ButtonOpts {
            text: bcd.values.get("description").unwrap(),
            width: 300 - 24,
            position: (12, (12 + i * 32) as i32),
            ..Default::default()
        });
        let identifier = bcd.values.get("identifier").unwrap().clone();

        btn.on().bn_clicked(move || {
            let modify_boot_sequence = Command::new("bcdedit")
                .args(&["/set", "{fwbootmgr}", "BootSequence", &identifier, "/AddFirst"])
                .output();

            match modify_boot_sequence {
                Ok(o) if o.status.success() => {
                    Command::new("shutdown")
                        .args(&["/r", "/f", "/t", "0"])
                        .status().unwrap();
                }
                Ok(o) => {
                    let err = String::from_utf8(o.stdout).unwrap_or_else(|_| "[could not parse utf8]".to_string());
                    wnd.hwnd().MessageBox(
                        &format!("bcdedit returned non-zero status: {}\n{}", o.status, err),
                        "Failed",
                        winsafe::co::MB::OK,
                    ).unwrap();
                    return Ok(());
                }
                Err(e) => {
                    wnd.hwnd().MessageBox(
                        &format!("failed to spawn bcdedit: {}", e),
                        "Failed",
                        winsafe::co::MB::OK,
                    ).unwrap();
                    return Ok(());
                }
            };

            wnd.close();

            Ok(())
        });
    }

    wnd.run_main(None).unwrap();

    Ok(())
}