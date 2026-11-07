# Keyboard reference

<!-- Made from the key map and the palette's list by crates/app/tests/reference.rs. Don't edit this page: change the code, then run `COLDOPEN_BLESS=1 cargo test -p coldopen-app --test reference`. -->

Every action is in the command palette (Ctrl+K, or Cmd+K on a Mac), with its keys beside it. This page lists them all, with the keys they have when Cold Open is new. You can change any of them in `keymap.json` (see [Your own keys](../guide/getting-started.md#your-own-keys)).

The keys that move the caret and edit text (the arrows, Home, End, Page Up and Down, Backspace, Delete, Enter and Tab) are built in: see [Getting started](../guide/getting-started.md).

## The app's actions

| Action | Name | Windows and Linux | macOS |
| --- | --- | --- | --- |
| Show the command palette | `app.palette` | `Ctrl+K`, `Ctrl+Shift+P` | `Cmd+K`, `Shift+Cmd+P` |
| New script | `app.new` | `Ctrl+N` | `Cmd+N` |
| Open a script | `app.open` | `Ctrl+O` | `Cmd+O` |
| Save as | `app.save_as` | `Ctrl+Shift+S` | `Shift+Cmd+S` |
| Save as Cold Open project | `app.save_as_project` | — | — |
| Close the script | `app.close` | `Ctrl+W` | `Cmd+W` |
| Export a PDF | `app.export_pdf` | `Ctrl+Shift+E` | `Shift+Cmd+E` |
| Save a copy as .fdx | `app.export_fdx` | — | — |
| Save a copy as Fountain | `app.export_fountain` | — | — |
| Repair an .fdx file | `app.repair` | — | — |
| Recovered versions | `app.recovered` | — | — |
| Quit | `app.quit` | `Ctrl+Q` | `Cmd+Q` |
| Undo agent edits | `app.undo_agent` | `Ctrl+Alt+Z` | `Option+Cmd+Z` |
| Show the undo history | `app.history` | `Ctrl+Shift+H` | `Shift+Cmd+H` |
| Turn agent access on or off | `app.agent_access` | — | — |
| Disconnect agents | `app.agent_disconnect` | — | — |
| Dismiss agent marks | `app.dismiss_marks` | — | — |
| Dismiss messages (also Esc) | `app.dismiss_messages` | — | — |
| Draw with the CPU (less memory; after a restart) | `app.renderer_cpu` | — | — |
| Draw with the GPU (smoother on 4K and Retina; after a restart) | `app.renderer_gpu` | — | — |
| About Cold Open and licenses | `app.about` | — | — |
| Measure performance | `app.bench` | — | — |
| Install the command-line tool (macOS only) | `app.install_cli` | — | — |
| Open the user guide | `app.user_guide` | — | — |
| Report a problem… | `app.report_problem` | — | — |
| Look at crash reports | `app.crash_reports` | — | — |
| Turn crash reports on or off | `app.crash_reports_toggle` | — | — |
| Crash on purpose (to test recovery) | `app.crash_test` | — | — |
| Settings… | `app.settings` | `Ctrl+,` | `Cmd+,` |
| Share feedback… | `app.share_feedback` | — | — |
| View usage summary | `app.usage_summary` | — | — |
| Turn usage counting on or off | `app.usage_toggle` | — | — |
| Cut | `edit.cut` | `Ctrl+X` | `Cmd+X` |
| Copy | `edit.copy` | `Ctrl+C` | `Cmd+C` |
| Paste | `edit.paste` | `Ctrl+V` | `Cmd+V` |
| Select all | `edit.select_all` | `Ctrl+A` | `Cmd+A` |
| Bold | `edit.bold` | `Ctrl+B` | `Cmd+B` |
| Italic | `edit.italic` | `Ctrl+I` | `Cmd+I` |
| Underline | `edit.underline` | `Ctrl+U` | `Cmd+U` |
| Find | `edit.find` | `Ctrl+F` | `Cmd+F` |
| Find and replace | `edit.replace` | `Ctrl+H` | `Option+Cmd+F` |
| Find next | `edit.find_next` | `F3`, `Ctrl+G` | `F3`, `Cmd+G` |
| Find previous | `edit.find_previous` | `Shift+F3`, `Ctrl+Shift+G` | `Shift+F3`, `Shift+Cmd+G` |
| Element: Scene Heading | `element.scene_heading` | `Ctrl+1` | `Cmd+1` |
| Element: Action | `element.action` | `Ctrl+2` | `Cmd+2` |
| Element: Character | `element.character` | `Ctrl+3` | `Cmd+3` |
| Element: Parenthetical | `element.parenthetical` | `Ctrl+4` | `Cmd+4` |
| Element: Dialogue | `element.dialogue` | `Ctrl+5` | `Cmd+5` |
| Element: Transition | `element.transition` | `Ctrl+6` | `Cmd+6` |
| Element: Shot | `element.shot` | `Ctrl+7` | `Cmd+7` |
| Element: General | `element.general` | `Ctrl+8` | `Cmd+8` |
| Element: Lyric | `element.lyric` | — | — |
| Toggle dual dialogue | `element.dual` | — | — |
| Zoom in | `view.zoom_in` | `Ctrl+=` | `Cmd+=` |
| Zoom out | `view.zoom_out` | `Ctrl+-` | `Cmd+-` |
| Actual size | `view.zoom_reset` | `Ctrl+0` | `Cmd+0` |
| Make the interface larger | `view.interface_larger` | — | — |
| Make the interface smaller | `view.interface_smaller` | — | — |
| Interface at normal size | `view.interface_reset` | — | — |
| Toggle the draft view | `view.draft` | `Ctrl+Shift+D` | `Shift+Cmd+D` |
| Show or hide the navigator | `view.navigator` | `Ctrl+\` | `Cmd+\` |
| Focus mode | `view.focus` | `Ctrl+Shift+F` | `Shift+Cmd+F` |
| Move focus to the next pane | `view.next_pane` | `F6` | `F6` |
| Theme: follow the system | `view.theme_system` | — | — |
| Theme: light | `view.theme_light` | — | — |
| Theme: dark | `view.theme_dark` | — | — |
| Theme: high contrast | `view.theme_high_contrast` | — | — |
| Go to the previous scene | `nav.previous_scene` | `Alt+Page Up` | `Option+Page Up` |
| Go to the next scene | `nav.next_scene` | `Alt+Page Down` | `Option+Page Down` |
| Move the current scene up | `nav.move_scene_up` | — | — |
| Move the current scene down | `nav.move_scene_down` | — | — |
| Accept the change at the caret | `changes.accept_here` | `Ctrl+Shift+Enter` | `Shift+Cmd+Enter` |
| Reject the change at the caret | `changes.reject_here` | `Ctrl+Shift+Backspace` | `Shift+Cmd+Backspace` |
| Go to the next change | `changes.next` | `F7` | `F7` |
| Go to the previous change | `changes.previous` | `Shift+F7` | `Shift+F7` |
| Accept the changes in this scene | `changes.accept_scene` | — | — |
| Reject the changes in this scene | `changes.reject_scene` | — | — |
| Accept all changes | `changes.accept_all` | — | — |
| Reject all changes | `changes.reject_all` | — | — |
| Track my changes (on or off) | `changes.track_mine` | — | — |
| Track agent edits to this script (on or off) | `changes.track_agents` | — | — |
| Show or hide the changes panel | `view.changes` | `Ctrl+Shift+R` | `Shift+Cmd+R` |
| Change my name for changes… | `app.author_name` | — | — |
| Reports… | `app.reports` | `Ctrl+Shift+I` | `Shift+Cmd+I` |
| Edit title page… | `app.title_page` | `Ctrl+Shift+T` | `Shift+Cmd+T` |
| Compare with… | `app.compare` | — | — |
| Review a draft as changes… | `app.review_draft` | — | — |
| Revision commands… | `app.revision_menu` | — | — |
| Start revision… | `app.revision_start` | — | — |
| End revision | `app.revision_end` | — | — |
| Revision colors… | `app.revision_colors` | — | — |
| Clear revision marks… | `app.revision_clear` | — | — |

## Commands

The commands of the [command reference](commands.md), as the palette lists them. One with parameters opens a small form for them.

| Action | Name | Windows and Linux | macOS |
| --- | --- | --- | --- |
| New document (params) | `doc.new` | — | — |
| Open a file by path | `doc.open` | — | — |
| Save | `doc.save` | `Ctrl+S` | `Cmd+S` |
| Close the document (params) | `doc.close` | — | — |
| Show the document as JSON | `doc.get` | — | — |
| Export as text | `doc.export` | — | — |
| Insert a block | `block.insert` | — | — |
| Change a block's element | `block.set_type` | — | — |
| Set a block's text | `block.set_text` | — | — |
| Set a block's attributes | `block.set_attrs` | — | — |
| Delete a block | `block.delete` | — | — |
| Replace text | `text.replace` | — | — |
| List scenes | `scene.list` | — | — |
| Move a scene | `scene.move` | — | — |
| Export a PDF by path | `export.pdf` | — | — |
| Undo | `undo` | `Ctrl+Z` | `Cmd+Z` |
| Redo | `redo` | `Ctrl+Shift+Z`, `Ctrl+Y` | `Shift+Cmd+Z` |
| Revert to the saved file | `doc.revert` | — | — |
| Insert text | `text.insert` | — | — |
| Delete text | `text.delete` | — | — |
| Style text | `text.style` | — | — |
| Split a block | `block.split` | — | — |
| Join a block to the one before | `block.join` | — | — |
| Run several edits as one | `batch` | — | — |
| Find text | `text.find` | — | — |
| Suggest completions | `text.complete` | — | — |
| List the undo history | `history.list` | — | — |
| Export an .fdx copy by path | `export.fdx` | — | — |
| Export a Fountain copy by path | `export.fountain` | — | — |
| Repair an .fdx file into a new one | `file.repair` | — | — |
| List recovered versions | `recovered.list` | — | — |
| Restore a recovered version | `recovered.restore` | — | — |
| Delete a recovered version | `recovered.delete` | — | — |
| List tracked changes | `changes.list` | — | — |
| Accept tracked changes by id, scene or all | `changes.accept` | — | — |
| Reject tracked changes by id, scene or all | `changes.reject` | — | — |
| Choose which edits are tracked | `changes.track` | — | — |
| Start a revision set | `revision.start` | — | — |
| End the revision set | `revision.end` | — | — |
| List revision sets | `revision.list` | — | — |
| Set the revision colors | `revision.colors` | — | — |
| Clear revision marks | `revision.clear` | — | — |
| Report: scenes | `report.scenes` | — | — |
| Report: characters | `report.characters` | — | — |
| Report: locations | `report.locations` | — | — |
| Show the title page as JSON | `title.get` | — | — |
| Set a title-page key (params) | `title.set` | — | — |
| Set the title page's lines (params) | `title.set_lines` | — | — |
| Compare two drafts (params) | `diff` | — | — |
| Bring a draft in as changes (params) | `changes.import` | — | — |
