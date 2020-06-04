This tool is now old and being rewritten for discord
====================================================

A bit of a hack to record stars on a slack server. Listens for a slack slash command and sends a
star.add command in response.

Usage
-----
1. Make a slack slash command on your team's slack pointing to where you will run the app.
   Take note of your api key and verification token.
2. Copy `config.toml.sample` to `config.toml` and fill in the fields with the above.
3. `cargo run`

To star something right click a line in slack, copy link and paste it with your slash command.
e.g. `/quoth https://hyenachat.slack.com/archives/general/p1482786363038760`

Known Issues
------------
Currently the path to the webapp is hardcoded as `localhost:8000/starlord`.
