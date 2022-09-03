use anyhow::{anyhow, bail, Result};
use show_image::{WindowOptions, WindowProxy, create_window, event::{ModifiersState, VirtualKeyCode, WindowEvent}};
use std::{io::Write, process::{Command, Stdio}};

pub fn show_graphviz(dot: &str) -> Result<()> {
    // Feed it into graphviz and then out into a PNG?

    let mut child = Command::new("dot")
        .arg("-Tpng")
        .arg("-Gdpi=150")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    child.stdin
        .as_mut()
        .ok_or(anyhow!("Child process stdin has not been captured!"))?
        .write_all(dot.as_bytes())?;

    let output = child.wait_with_output()?;

    if !output.status.success() {
        bail!("Couldn't run graphviz. Ensure `dot` is on your path.");
    }

    let im = image::load_from_memory_with_format(
        &output.stdout,
        image::ImageFormat::Png,
    )?;

    // Create a window with default options and display the image.
    let window = create_window(
        "Build DAG",
        WindowOptions {
            size: Some([1500, 1000]),
            ..Default::default()
        },
    )?;
    window.set_image("image-001", im)?;

    wait_for_window(&window)?;
    Ok(())

}


fn wait_for_window(window: &WindowProxy)-> Result<()> {
    	// Wait for the window to be closed or Escape to be pressed.
	for event in window.event_channel()? {
		if let WindowEvent::KeyboardInput(event) = event {
			if event.is_synthetic || !event.input.state.is_pressed() {
				continue;
			}
			if event.input.key_code == Some(VirtualKeyCode::Escape) {
				println!("Escape pressed!");
				break;
			} else if event.input.key_code == Some(VirtualKeyCode::O) && event.input.modifiers == ModifiersState::CTRL {
				println!("Ctrl+O pressed, toggling overlay");
				// window.run_function_wait(|mut window| {
				// 	window.set_overlays_visible(!window.overlays_visible());
				// });
			}
		}
	}
    Ok(())
}
