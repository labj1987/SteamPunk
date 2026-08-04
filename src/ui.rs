use crate::launcher::{self, LaunchTarget};
use crate::library::{self, Trainer};

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DropTarget, FileDialog, FileFilter, Label, ListBox, Orientation,
    ScrolledWindow, SelectionMode, Stack,
};
use libadwaita::prelude::*;
use libadwaita::{
    ActionRow, AlertDialog, Application, ApplicationWindow, HeaderBar, StatusPage, Toast,
    ToastOverlay, Window as AdwWindow,
};

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

// ─────────────────────────────────────────────────────────────────────────────
//  Async bridge: run a future on the shared Tokio runtime, deliver the result
//  back on the GTK main thread.
// ─────────────────────────────────────────────────────────────────────────────

fn spawn_async<F, T, CB>(future: F, callback: CB)
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
    CB: FnOnce(T) + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel::<T>();
    crate::runtime().spawn(async move {
        let _ = tx.send(future.await);
    });
    glib::MainContext::default().spawn_local(async move {
        if let Ok(val) = rx.await {
            callback(val);
        }
    });
}

fn log_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/share/proton-trainer/launch.log")
}

// ─────────────────────────────────────────────────────────────────────────────
//  build_ui
// ─────────────────────────────────────────────────────────────────────────────

pub fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Proton Trainer")
        .default_width(480)
        .default_height(560)
        .build();

    let toast_overlay = ToastOverlay::new();

    // ── Header ───────────────────────────────────────────────────────────────
    let header = HeaderBar::new();

    let import_btn = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Import trainer .exe")
        .build();
    header.pack_start(&import_btn);

    let stop_all_btn = Button::builder().label("Stop All").build();
    header.pack_start(&stop_all_btn);

    let about_btn = Button::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text("About")
        .build();
    header.pack_end(&about_btn);

    let troubleshoot_btn = Button::builder().label("Fix Stale Instance").build();
    header.pack_end(&troubleshoot_btn);

    // ── Stack: empty status page <-> trainer list ───────────────────────────
    let stack = Stack::new();

    let status_page = StatusPage::builder()
        .icon_name("input-gaming-symbolic")
        .title("No Trainers Yet")
        .description("Drop a trainer .exe here, or use the + button")
        .build();
    stack.add_named(&status_page, Some("empty"));

    let list_box = ListBox::new();
    list_box.set_selection_mode(SelectionMode::None);
    list_box.add_css_class("boxed-list");
    list_box.set_margin_top(12);
    list_box.set_margin_bottom(12);
    list_box.set_margin_start(12);
    list_box.set_margin_end(12);
    let list_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .child(&list_box)
        .build();
    stack.add_named(&list_scroll, Some("list"));

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&header);
    content.append(&stack);
    toast_overlay.set_child(Some(&content));
    window.set_content(Some(&toast_overlay));

    // ─────────────────────────────────────────────────────────────────────────
    //  refresh_list — re-read the trainers dir and rebuild the list/rows
    // ─────────────────────────────────────────────────────────────────────────

    // Forward-declaration slot so the Remove button (defined inside the
    // closure being built) can call refresh_list once it exists.
    let self_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let refresh_list: Rc<dyn Fn()> = {
        let list_box = list_box.clone();
        let stack = stack.clone();
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        let self_slot = self_slot.clone();

        Rc::new(move || {
            while let Some(c) = list_box.first_child() {
                list_box.remove(&c);
            }

            let trainers = library::list_trainers().unwrap_or_default();
            if trainers.is_empty() {
                stack.set_visible_child_name("empty");
                return;
            }
            stack.set_visible_child_name("list");

            for trainer in trainers {
                let row = ActionRow::builder().title(&trainer.name).build();

                let launch_btn = Button::builder()
                    .label("Launch")
                    .valign(Align::Center)
                    .build();
                launch_btn.add_css_class("suggested-action");
                wire_launch_button(&launch_btn, trainer.clone(), window.clone(), toast_overlay.clone());
                row.add_suffix(&launch_btn);

                let remove_btn = Button::builder()
                    .icon_name("user-trash-symbolic")
                    .tooltip_text("Remove")
                    .valign(Align::Center)
                    .build();
                remove_btn.add_css_class("flat");
                {
                    let trainer = trainer.clone();
                    let toast_overlay = toast_overlay.clone();
                    let self_slot = self_slot.clone();
                    remove_btn.connect_clicked(move |_| {
                        if let Err(e) = library::remove_trainer(&trainer.path) {
                            toast_overlay.add_toast(Toast::new(&format!("Remove failed: {e}")));
                            return;
                        }
                        if let Some(reload) = self_slot.borrow().clone() {
                            reload();
                        }
                    });
                }
                row.add_suffix(&remove_btn);

                list_box.append(&row);
            }
        })
    };
    *self_slot.borrow_mut() = Some(refresh_list.clone());

    refresh_list();

    // ─────────────────────────────────────────────────────────────────────────
    //  Import — file picker
    // ─────────────────────────────────────────────────────────────────────────
    {
        let window = window.clone();
        let refresh_list = refresh_list.clone();
        let toast_overlay = toast_overlay.clone();
        import_btn.connect_clicked(move |_| {
            let filter = FileFilter::new();
            filter.add_pattern("*.exe");
            filter.set_name(Some("Trainer executable (.exe)"));
            let filters = gio::ListStore::new::<FileFilter>();
            filters.append(&filter);

            let dialog = FileDialog::builder()
                .title("Import Trainer")
                .filters(&filters)
                .build();

            let refresh_list = refresh_list.clone();
            let toast_overlay = toast_overlay.clone();
            dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else { return };
                let Some(path) = file.path() else { return };
                match library::import_trainer(&path) {
                    Ok(_) => {
                        refresh_list();
                        toast_overlay.add_toast(Toast::new("Trainer imported"));
                    }
                    Err(e) => toast_overlay.add_toast(Toast::new(&format!("Import failed: {e}"))),
                }
            });
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  Drop target — accept dropped .exe files anywhere in the window
    // ─────────────────────────────────────────────────────────────────────────
    {
        let drop_target = DropTarget::new(gdk4::FileList::static_type(), gdk4::DragAction::COPY);
        let refresh_list = refresh_list.clone();
        let toast_overlay = toast_overlay.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            let Ok(file_list) = value.get::<gdk4::FileList>() else {
                return false;
            };
            let mut imported_any = false;
            for file in file_list.files() {
                let Some(path) = file.path() else { continue };
                let is_exe = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("exe"))
                    .unwrap_or(false);
                if !is_exe {
                    continue;
                }
                match library::import_trainer(&path) {
                    Ok(_) => imported_any = true,
                    Err(e) => toast_overlay.add_toast(Toast::new(&format!("Import failed: {e}"))),
                }
            }
            if imported_any {
                refresh_list();
                toast_overlay.add_toast(Toast::new("Trainer imported"));
            }
            imported_any
        });
        content.add_controller(drop_target);
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  Stop All
    // ─────────────────────────────────────────────────────────────────────────
    {
        let toast_overlay = toast_overlay.clone();
        stop_all_btn.connect_clicked(move |_| {
            let toast_overlay = toast_overlay.clone();
            spawn_async(
                async {
                    tokio::task::spawn_blocking(|| {
                        if let Ok(dir) = library::trainers_dir() {
                            launcher::stop_all(&dir);
                        }
                    })
                    .await
                },
                move |_| {
                    toast_overlay.add_toast(Toast::new("Stopped all trainers"));
                },
            );
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  Fix Stale Instance
    // ─────────────────────────────────────────────────────────────────────────
    {
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        troubleshoot_btn.connect_clicked(move |_| {
            show_troubleshoot(&window, &toast_overlay);
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  About
    // ─────────────────────────────────────────────────────────────────────────
    {
        let window = window.clone();
        about_btn.connect_clicked(move |_| {
            let dialog = gtk4::AboutDialog::builder()
                .program_name("Proton Trainer")
                .version(env!("CARGO_PKG_VERSION"))
                .authors(vec!["Linnard Alex Brown Jr.".to_string()])
                .comments(
                    "Launches Windows game trainers through Proton against a running \
                     Steam game's wine session.",
                )
                .build();
            dialog.set_transient_for(Some(&window));
            dialog.present();
        });
    }

    window.present();
}

// ─────────────────────────────────────────────────────────────────────────────
//  Launch button wiring
// ─────────────────────────────────────────────────────────────────────────────

fn wire_launch_button(
    btn: &Button,
    trainer: Trainer,
    window: ApplicationWindow,
    toast_overlay: ToastOverlay,
) {
    btn.connect_clicked(move |_| {
        let trainer = trainer.clone();
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();

        spawn_async(
            async { tokio::task::spawn_blocking(launcher::resolve_launch_target).await },
            move |result| {
                let target = match result {
                    Ok(Ok(t)) => t,
                    Ok(Err(e)) => {
                        toast_overlay.add_toast(Toast::new(&e.to_string()));
                        return;
                    }
                    Err(e) => {
                        toast_overlay.add_toast(Toast::new(&format!("Task error: {e}")));
                        return;
                    }
                };

                if !launcher::has_dotnet40(&target) {
                    show_dotnet_dialog(&window, &target);
                    return;
                }

                let trainer_name = trainer.name.clone();
                let trainer_path = trainer.path.clone();
                let toast_overlay2 = toast_overlay.clone();
                let log = log_path();
                let appid = target.appid;

                spawn_async(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            launcher::launch_trainer(&target, &trainer_path, &log)
                        })
                        .await
                    },
                    move |launch_result| match launch_result {
                        Ok(Ok(())) => toast_overlay2.add_toast(Toast::new(&format!(
                            "Launched {trainer_name} (AppId {appid})"
                        ))),
                        Ok(Err(e)) => {
                            toast_overlay2.add_toast(Toast::new(&format!("Launch failed: {e}")))
                        }
                        Err(e) => {
                            toast_overlay2.add_toast(Toast::new(&format!("Task error: {e}")))
                        }
                    },
                );
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
//  .NET 4.0 one-time setup dialog
// ─────────────────────────────────────────────────────────────────────────────

fn show_dotnet_dialog(window: &ApplicationWindow, target: &LaunchTarget) {
    let wineprefix = target.prefix_dir();
    let body = format!(
        "FLiNG trainers are WPF apps and crash under Proton's bundled wine-mono. \
This game's prefix needs real .NET Framework 4.0, installed once.\n\
\n\
One-time system packages (as root):\n\
  dpkg --add-architecture i386 && apt update && apt install -y winetricks cabextract wine32:i386\n\
\n\
With the game CLOSED (no wineserver running), as your normal user:\n\
  WINEPREFIX={} winetricks -q dotnet40 win10\n\
\n\
Two constraints that matter:\n\
1. The install must use system wine with wine32:i386 present (classic mode). Installing with \
Proton's own wine, or wine's wow64 mode without wine32, fails with a known \"FDICopy failed\" \
bug on netfx_core.mzz.\n\
2. The prefix must be one Proton itself created. If this prefix is broken or fresh: delete its \
compatdata, launch the game once via Steam so Proton builds a native prefix, quit, then run the \
winetricks command above. Installing into a winetricks-created prefix and letting Proton adopt \
it breaks SteamAPI init.\n\
\n\
After this, Proton's xalia.exe accessibility helper may show a harmless wine \"Program Error\" \
dialog at the next game launch — just close it.",
        wineprefix.display()
    );

    let dialog = AlertDialog::builder()
        .heading("One-Time .NET 4.0 Setup Needed")
        .body(&body)
        .build();
    dialog.add_responses(&[("ok", "Got It")]);
    dialog.set_default_response(Some("ok"));
    dialog.present(Some(window));
}

// ─────────────────────────────────────────────────────────────────────────────
//  Stale-instance recovery dialog
// ─────────────────────────────────────────────────────────────────────────────

fn show_troubleshoot(window: &ApplicationWindow, toast_overlay: &ToastOverlay) {
    let window = window.clone();
    let toast_overlay = toast_overlay.clone();

    spawn_async(
        async { tokio::task::spawn_blocking(launcher::resolve_launch_target).await },
        move |result| {
            let target = match result {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    toast_overlay.add_toast(Toast::new(&e.to_string()));
                    return;
                }
                Err(e) => {
                    toast_overlay.add_toast(Toast::new(&format!("Task error: {e}")));
                    return;
                }
            };

            let dirs = launcher::trainer_log_dirs(&target);
            if dirs.is_empty() {
                toast_overlay.add_toast(Toast::new("No trainer logs found for the running game"));
                return;
            }

            let dialog = AdwWindow::builder()
                .transient_for(&window)
                .modal(true)
                .title("Clear Stale Trainer Instance")
                .default_width(420)
                .default_height(320)
                .build();

            let outer = GtkBox::new(Orientation::Vertical, 8);
            outer.set_margin_top(12);
            outer.set_margin_bottom(12);
            outer.set_margin_start(12);
            outer.set_margin_end(12);

            let hint = Label::new(Some(
                "If a trainer refuses to start, claiming a previous instance is running, \
                 clear its log folder below, then relaunch.",
            ));
            hint.set_wrap(true);
            hint.set_xalign(0.0);
            outer.append(&hint);

            let list = ListBox::new();
            list.set_selection_mode(SelectionMode::None);
            list.add_css_class("boxed-list");
            outer.append(&list);

            for dir in dirs {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let row = ActionRow::builder().title(&name).build();

                let clear_btn = Button::builder()
                    .label("Clear")
                    .valign(Align::Center)
                    .build();
                clear_btn.add_css_class("destructive-action");

                let toast_overlay2 = toast_overlay.clone();
                let dir2 = dir.clone();
                clear_btn.connect_clicked(move |_| {
                    match std::fs::remove_file(dir2.join("info.ini")) {
                        Ok(()) => toast_overlay2.add_toast(Toast::new("Cleared")),
                        Err(e) => {
                            toast_overlay2.add_toast(Toast::new(&format!("Failed: {e}")))
                        }
                    }
                });
                row.add_suffix(&clear_btn);
                list.append(&row);
            }

            let scroll = ScrolledWindow::builder().vexpand(true).child(&outer).build();
            dialog.set_content(Some(&scroll));
            dialog.present();
        },
    );
}
