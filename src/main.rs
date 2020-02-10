extern crate counter;
extern crate i3ipc;
extern crate signal_hook;
#[macro_use(lazy_static)]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate simple_logger;

use counter::Counter;
use i3ipc::I3Connection;
use log::{info, trace, warn};
use regex::Regex;
use signal_hook::{iterator::Signals, SIGINT, SIGTERM};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::vec::Vec;
use std::{error::Error, thread, time::Duration};

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

fn main() {
    simple_logger::init().unwrap();

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

    // TODO: callback
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
        info!("rename workspace {} to {}", workspace.name, new_name);
        // c.run_command(format!("rename workspace {} to {}", workspace.name, new_name).as_str());
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
    info!("name: {}", name);
    let re = Regex::new(r"(?P<num>\d+):?(?P<shortname>-u:\w)? ?(?P<icons>.+)?").unwrap();
    let matches = re.captures(name);
    info!(
        "iter length: {:?}",
        re.captures_iter(name)
            .into_iter()
            .collect::<Vec<regex::Captures>>()
            .len()
    );
    // info!("match is: {}", matches[<F12>0]);
    // TODO: distinguish None?
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
            [np.shortname.as_ref().unwrap_or(&String::from("")).as_str(), " ", np.icons.as_ref().unwrap().as_str()].concat()
        } else {
            String::from(np.shortname.as_ref().unwrap_or(&String::from("")).as_str())
        }
    } else {
        String::from(" ")
    };

    return [first_part, last_part].concat();
}

fn rename_workspaces(con: Arc<Mutex<I3Connection>>) -> Result<(), i3ipc::MessageError> {
    let mut c = con.lock().unwrap();
    let ws_infos = (c.get_workspaces()?).workspaces;
    let mut prev_output: Option<String> = None;
    let mut n: u32 = 1;
    let tree = c.get_tree()?;
    let workspaces = find_focused_workspace(&tree);

    for (ws_index, workspace) in workspaces.iter().enumerate() {
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

        // TODO: del
        let foo = &name_parts.clone();
        info!("{:?}", foo);

        // TODO: renumber workspaces
        let new_num = name_parts.num;
        n += 1;

        let new_name = construct_workspace_name(&NameParts {
            num: new_num,
            shortname: name_parts.shortname,
            icons: Some(new_icons),
        });

        match workspace.name.as_ref() {
            Some(n) => {
                info!("rename workspace {} to {}", n, new_name);
                // c.run_command(format!("rename workspace {} to {}", n, new_name).as_str())?;
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
    let classes = node.window.and_then(|w| xprop(w, "WM_CLASS"));
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

fn xprop(win_id: i32, property: &str) -> Option<Vec<String>> {
    return Command::new("xprop")
        .arg("-id")
        .arg(win_id.to_string())
        .arg(property)
        .output()
        .ok()
        .and_then(|r| String::from_utf8(r.stdout).ok())
        .map(|prop| {
            let re = Regex::new(r#"([^"]*)"#).unwrap();
            return re
                .find_iter(prop.as_str())
                .map(|m| String::from(m.as_str()))
                .collect::<Vec<String>>();
        });
}

// TODO: support for superscript and normal numbers
fn format_icon_list(icons: Vec<String>) -> String {
    let mut new_list: Vec<String> = Vec::new();
    let icon_count = icons.into_iter().collect::<Counter<_>>();
    for (icon, count) in icon_count.iter() {
        if *count > 1 {
            new_list.push(
                [
                    icon.to_string(),
                    " ".to_string(),
                    encode_base_10_number(*count as usize, SUPERSCRIPT),
                ]
                .concat(),
            );
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
