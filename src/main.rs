extern crate counter;
extern crate i3ipc;
extern crate signal_hook;
#[macro_use(lazy_static)]
extern crate lazy_static;
extern crate log;
extern crate simple_logger;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate clap;
extern crate xcb;

use counter::Counter;
use errors::*;
use i3ipc::I3Connection;
use log::{info, warn};
use regex::Regex;
use signal_hook::{iterator::Signals, SIGINT, SIGTERM};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::vec::Vec;

mod errors;

#[derive(Debug)]
pub enum IconListFormat {
    Superscript,
    Subscript,
    Digits,
}

impl FromStr for IconListFormat {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<IconListFormat, ()> {
        match s.to_lowercase().as_str() {
            "superscript" => Ok(IconListFormat::Superscript),
            "subscript" => Ok(IconListFormat::Subscript),
            "digits" => Ok(IconListFormat::Digits),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub struct Settings {
    pub icon_list_format: IconListFormat,
    pub renumber_workspaces: bool,
}

// emulated global variable for our settings
lazy_static! {
    pub static ref SETTINGS: Mutex<Settings> = Mutex::new(Settings {
        icon_list_format: IconListFormat::Digits,
        renumber_workspaces: false
    });
}

const SUPERSCRIPT: &'static [&'static str; 10] =
    &["⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"];
const SUBSCRIPT: &'static [&'static str; 10] = &["₀", "₁", "₂", "₃", "₄", "₅", "₆", "₇", "₈", "₉"];
const DIGITS: &'static [&'static str; 10] = &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

lazy_static! {
    static ref WINDOW_ICONS: HashMap<&'static str, &'static str> = vec![
        ("NeovimGtk", "\u{f121}"),
        ("alacritty", "\u{f120}"),
        ("atom", "\u{f121}"),
        ("banshee", "\u{f04b}"),
        ("blender", "\u{f1b2}"),
        ("chromium", "\u{f268}"),
        ("cura", "\u{f1b2}"),
        ("darktable", "\u{f03e}"),
        ("discord", "\u{f075}"),
        ("eclipse", "\u{f121}"),
        ("emacs", "\u{f121}"),
        ("eog", "\u{f03e}"),
        ("evince", "\u{f1c1}"),
        ("evolution", "\u{f0e0}"),
        ("feh", "\u{f03e}"),
        ("file-roller", "\u{f066}"),
        ("filezilla", "\u{f233}"),
        ("firefox", "\u{f269}"),
        ("firefox-esr", "\u{f269}"),
        ("gimp-2.8", "\u{f03e}"),
        ("git-gui", "\u{f1d3}"),
        ("gitk", "\u{f1d3}"),
        ("gnome-control-center", "\u{f205}"),
        ("gnome-terminal-server", "\u{f120}"),
        ("google-chrome", "\u{f268}"),
        ("gpick", "\u{f1fb}"),
        ("gvim", "\u{f121}"),
        ("imv", "\u{f03e}"),
        ("java", "\u{f121}"),
        ("jetbrains-idea", "\u{f121}"),
        ("jetbrains-studio", "\u{f121}"),
        ("keepassxc", "\u{f084}"),
        ("keybase", "\u{f084}"),
        ("kicad", "\u{f2db}"),
        ("kitty", "\u{f120}"),
        ("libreoffice", "\u{f15c}"),
        ("lua5.1", "\u{f186}"),
        ("mpv", "\u{f26c}"),
        ("mupdf", "\u{f1c1}"),
        ("mysql-workbench-bin", "\u{f1c0}"),
        ("nautilus", "\u{f0c5}"),
        ("nemo", "\u{f0c5}"),
        ("openscad", "\u{f1b2}"),
        ("pavucontrol", "\u{f028}"),
        ("pidgin", "\u{f075}"),
        ("postman", "\u{f197}"),
        ("rhythmbox", "\u{f04b}"),
        ("robo3t", "\u{f1c0}"),
        ("sakura", "\u{f120}"),
        ("slack", "\u{f198}"),
        ("slic3r.pl", "\u{f1b2}"),
        ("spacefm", "\u{f0c5}"),
        ("spotify", "\u{f001}"),
        ("steam", "\u{f1b6}"),
        ("subl", "\u{f15c}"),
        ("subl3", "\u{f15c}"),
        ("sublime_text", "\u{f15c}"),
        ("thunar", "\u{f0c5}"),
        ("thunderbird", "\u{f0e0}"),
        ("totem", "\u{f04b}"),
        ("urxvt", "\u{f120}"),
        ("vim", "\u{f121}"),
        ("xfce4-terminal", "\u{f120}"),
        ("xournal", "\u{f15c}"),
        ("yelp", "\u{f121}"),
        ("zenity", "\u{f2d0}"),
        ("zoom", "\u{f075}"),
    ]
    .into_iter()
    .collect();
}

fn main() -> Result<()> {
    let mut app = clap_app!(rustygitprompt =>
        (version: "1.0")
        (author: "Julian Ospald <hasufell@posteo.de>")
        (about: "Polybar/i3 workspace formatter")
        (@arg FORMAT: -i --icon_list_format +takes_value "Sets the format for icon list: superscript, subscript or digits (default)")
        (@arg RENUMBER_WORKSPACES: -r --renumber_workspaces "Whether to renumber the workspaces (default: false)")
        (@arg debug: -d "Whether to print debugs (default: false)")
    );

    let format = app
        .clone()
        .get_matches()
        .value_of("FORMAT")
        .map(|s| s.to_string());
    let renum = app.clone().get_matches().is_present("RENUMBER_WORKSPACES");
    let debug = app.clone().get_matches().is_present("debug");

    if let Some(c) = format {
        if let Ok(f) = IconListFormat::from_str(&c) {
            let mut settings = SETTINGS.lock().unwrap();
            settings.icon_list_format = f;
        } else {
            let _ = app.print_help();
            println!("");
            std::process::exit(1);
        }
    };

    {
        let mut settings = SETTINGS.lock().unwrap();
        settings.renumber_workspaces = renum;
    }

    if debug {
        simple_logger::init().unwrap();
    }

    // establish a connection to i3 over a unix socket
    let connection = Arc::new(Mutex::new(I3Connection::connect().unwrap()));

    let signals = Signals::new(&[SIGINT, SIGTERM]).unwrap();

    let con = Arc::clone(&connection);
    thread::spawn(move || {
        for sig in signals.forever() {
            match sig {
                signal_hook::SIGINT => on_exit(con.clone()),
                signal_hook::SIGTERM => on_exit(con.clone()),
                _ => unreachable!(),
            }
        }
    });

    match rename_workspaces(connection.clone()) {
        Ok(_) => info!("Successfully renamed workspaces"),
        Err(err) => warn!("Error renaming workspaces: {}", err),
    }

    let mut event_listener = i3ipc::I3EventListener::connect().unwrap();

    event_listener.subscribe(&[i3ipc::Subscription::Workspace, i3ipc::Subscription::Window])?;

    for event in event_listener.listen() {
        match event.as_ref() {
            Ok(i3ipc::event::Event::WindowEvent(info)) => match info.change {
                i3ipc::event::inner::WindowChange::New => {
                    rename_workspaces_report(connection.clone())
                }
                i3ipc::event::inner::WindowChange::Close => {
                    rename_workspaces_report(connection.clone())
                }
                i3ipc::event::inner::WindowChange::Move => {
                    rename_workspaces_report(connection.clone())
                }
                _ => (),
            },
            Ok(i3ipc::event::Event::WorkspaceEvent(info)) => match info.change {
                i3ipc::event::inner::WorkspaceChange::Move => {
                    rename_workspaces_report(connection.clone())
                }
                i3ipc::event::inner::WorkspaceChange::Init => match on_init(connection.clone()) {
                    Ok(_) => (),
                    Err(e) => warn!("Error on initialisation: {}", e),
                },
                _ => (),
            },
            Err(err) => warn!("Error: {}", err),
            _ => (),
        }
    }

    return Ok(());
}

fn on_init(con: Arc<Mutex<I3Connection>>) -> Result<()> {
    let mut c = con.lock().unwrap();
    let tree = c.get_tree()?;
    let ws = find_focused_workspace(&tree).unwrap();
    let name_parts = parse_workspace_name(ws.name.as_ref().unwrap().as_str()).unwrap();
    let new_name = construct_workspace_name(&NameParts {
        num: name_parts.num,
        shortname: None,
        icons: None,
    });
    c.run_command(
        format!(
            "rename workspace \"{}\" to \"{}\"",
            ws.name.as_ref().unwrap(),
            new_name
        )
        .as_str(),
    )?;

    return Ok(());
}

fn on_exit(con: Arc<Mutex<I3Connection>>) {
    let mut c = con.lock().unwrap();
    let ws = c
        .get_workspaces()
        .unwrap_or(i3ipc::reply::Workspaces {
            workspaces: Vec::new(),
        })
        .workspaces;
    let mut i: u32 = 1;

    for workspace in ws {
        let name_parts = match parse_workspace_name(workspace.name.as_str()) {
            Some(np) => np,
            None => NameParts {
                num: Some(i.to_string()),
                shortname: None,
                icons: None,
            },
        };
        let new_name: String = construct_workspace_name(&name_parts);
        i += 1;

        if workspace.name == new_name {
            continue;
        }
        info!(
            "rename workspace \"{}\" to \"{}\"",
            workspace.name, new_name
        );
        match c.run_command(
            format!(
                "rename workspace \"{}\" to \"{}\"",
                workspace.name, new_name
            )
            .as_str(),
        ) {
            Ok(_) => (),
            Err(err) => warn!("Error running command: {}", err),
        }
    }

    std::process::exit(0);
}

#[derive(Debug, Clone)]
struct NameParts {
    num: Option<String>,
    shortname: Option<String>,
    icons: Option<String>,
}

fn parse_workspace_name(name: &str) -> Option<NameParts> {
    let re = Regex::new(r"(?P<num>\d+):?(?P<shortname>-u:\w)? ?(?P<icons>.+)?").unwrap();
    let matches = re.captures(name);
    match matches {
        Some(m) => {
            return Some(NameParts {
                num: m.get(1).map(|m| String::from(m.as_str())),
                shortname: m.get(2).map(|m| String::from(m.as_str())),
                icons: m.get(3).map(|m| String::from(m.as_str())),
            });
        }
        None => return None,
    }
}

fn construct_workspace_name(np: &NameParts) -> String {
    let first_part = [np.num.as_ref().unwrap().as_str(), ":"].concat();
    let last_part = if np.shortname.is_some() || np.icons.is_some() {
        if np.icons.is_some() {
            [
                np.shortname.as_ref().unwrap_or(&String::from("")).as_str(),
                " ",
                np.icons.as_ref().unwrap().as_str(),
            ]
            .concat()
        } else {
            String::from(np.shortname.as_ref().unwrap_or(&String::from("")).as_str())
        }
    } else {
        String::from(" ")
    };

    return [first_part, last_part].concat();
}

fn rename_workspaces_report(con: Arc<Mutex<I3Connection>>) {
    match rename_workspaces(con) {
        Ok(_) => info!("Successfully renamed workspaces"),
        Err(err) => warn!("Error renaming workspaces: {}", err),
    }
}

fn rename_workspaces(con: Arc<Mutex<I3Connection>>) -> Result<()> {
    let mut c = con.lock().unwrap();
    let ws_infos = (c.get_workspaces()?).workspaces;
    let mut prev_output: Option<String> = None;
    let mut n: u32 = 1;
    let tree = c.get_tree()?;
    let workspaces: Vec<&i3ipc::reply::Node> = find_workspaces(&tree);

    for (ws_index, workspace) in workspaces.iter().enumerate() {
        if ws_index >= ws_infos.len() {
            break;
        }
        let ws_info = &ws_infos[ws_index];
        let name_parts = match workspace
            .name
            .as_ref()
            .and_then(|n| parse_workspace_name(n.as_str()))
        {
            Some(n) => n,
            None => NameParts {
                num: Some(n.to_string()),
                shortname: None,
                icons: None,
            },
        };
        let mut icon_list: Vec<String> = Vec::new();
        for leave in leaves(workspace) {
            icon_list.push(icon_for_window(leave));
        }
        let new_icons = format_icon_list(icon_list);

        match prev_output.as_ref() {
            Some(o) => {
                if ws_info.output != *o {
                    n += 1;
                }
            }
            _ => (),
        }
        prev_output = Some(ws_info.output.clone());

        // TODO: renumber workspaces
        let settings = SETTINGS.lock().unwrap();
        let renum = settings.renumber_workspaces;
        let new_num = if renum {
            Some(n.to_string())
        } else {
            name_parts.num
        };
        n += 1;

        let new_name = construct_workspace_name(&NameParts {
            num: new_num,
            shortname: name_parts.shortname,
            icons: Some(new_icons),
        });

        match workspace.name.as_ref() {
            Some(n) => {
                info!("rename workspace \"{}\" to \"{}\"", n, new_name);
                match c
                    .run_command(format!("rename workspace \"{}\" to \"{}\"", n, new_name).as_str())
                {
                    Ok(_) => (),
                    Err(err) => warn!("Error running command: {}", err),
                }
            }
            None => warn!("Could not find workspace name"),
        }
    }

    return Ok(());
}

fn find_focused_workspace<'a>(node: &'a i3ipc::reply::Node) -> Option<&'a i3ipc::reply::Node> {
    let mut work_node: Option<&'a i3ipc::reply::Node> = None;
    return find_focused_workspace_rec(node, &mut work_node);
}

fn find_focused_workspace_rec<'a>(
    node: &'a i3ipc::reply::Node,
    work_node: &mut Option<&'a i3ipc::reply::Node>,
) -> Option<&'a i3ipc::reply::Node> {
    if node.nodetype == i3ipc::reply::NodeType::Workspace {
        *work_node = Some(node);
    }

    if node.focused {
        return *work_node;
    } else {
        if let Some(&want) = node.focus.get(0) {
            let child = node.nodes.iter().find(|n| want == n.id).unwrap();
            return find_focused_workspace_rec(child, work_node);
        } else {
            return None;
        }
    }
}

// Find workspaces from this node. Ignored nodes without percentage (e.g. root or scratchpad).
fn find_workspaces(node: &i3ipc::reply::Node) -> Vec<&i3ipc::reply::Node> {
    let mut ws = Vec::new();
    find_workspaces_rec(node, &mut ws);
    return ws;
}

fn find_workspaces_rec<'a>(node: &'a i3ipc::reply::Node, ws: &mut Vec<&'a i3ipc::reply::Node>) {
    if node.nodetype == i3ipc::reply::NodeType::Workspace
        && node.name != Some("__i3_scratch".to_string())
    {
        ws.push(node);
    }

    for child in node.nodes.iter() {
        find_workspaces_rec(child, ws);
    }
}

fn leaves(node: &i3ipc::reply::Node) -> Vec<&i3ipc::reply::Node> {
    let mut vec: Vec<&i3ipc::reply::Node> = Vec::new();
    for n in &node.nodes {
        if n.nodes.is_empty() {
            vec.push(&n);
        } else {
            let child_leaves = leaves(&n);
            vec.extend(child_leaves);
        }
    }

    return vec;
}

fn icon_for_window(node: &i3ipc::reply::Node) -> String {
    let (conn, _) = xcb::Connection::connect(None).unwrap();
    let classes = node.window.and_then(|w| Some(get_wm_classes(&conn, &w)));
    match classes {
        Some(c) => {
            if c.len() > 0 {
                for class in c {
                    match WINDOW_ICONS.get(class.to_ascii_lowercase().as_str()) {
                        Some(m) => return String::from(*m),
                        None => (),
                    }
                }
            }
            return String::from("*");
        }
        None => return String::from("*"),
    }
}

fn format_icon_list(icons: Vec<String>) -> String {
    let mut new_list: Vec<String> = Vec::new();
    let icon_count = icons.into_iter().collect::<Counter<_>>();
    for (icon, count) in icon_count.iter() {
        if *count > 1 {
            let settings = SETTINGS.lock().unwrap();
            let encode_number = match &settings.icon_list_format {
                IconListFormat::Superscript => encode_base_10_number(*count as usize, SUPERSCRIPT),
                IconListFormat::Subscript => encode_base_10_number(*count as usize, SUBSCRIPT),
                IconListFormat::Digits => encode_base_10_number(*count as usize, DIGITS),
            };
            new_list.push([icon.to_string(), encode_number].concat());
        } else {
            new_list.push(icon.to_string());
        }
    }

    return new_list.join(" ");
}

fn encode_base_10_number(n: usize, symbols: &[&str; 10]) -> String {
    n.to_string()
        .chars()
        .map(|c| symbols[c.to_digit(10).unwrap() as usize])
        .collect()
}

fn get_wm_classes(conn: &xcb::Connection, id: &i32) -> Vec<String> {
    let window: xcb::xproto::Window = *id as u32;
    let long_length: u32 = 8;
    let mut long_offset: u32 = 0;
    let mut buf = Vec::new();
    loop {
        let cookie = xcb::xproto::get_property(
            &conn,
            false,
            window,
            xcb::xproto::ATOM_WM_CLASS,
            xcb::xproto::ATOM_STRING,
            long_offset,
            long_length,
        );
        match cookie.get_reply() {
            Ok(reply) => {
                let value: &[u8] = reply.value();
                buf.extend_from_slice(value);
                match reply.bytes_after() {
                    0 => break,
                    _ => {
                        let len = reply.value_len();
                        long_offset += len / 4;
                    }
                }
            }
            Err(err) => {
                println!("{:?}", err);
                break;
            }
        }
    }
    let result = String::from_utf8(buf).unwrap();
    let results: Vec<&str> = result.split('\0').collect();

    return results.iter().map(|r| r.to_string()).collect();
}
