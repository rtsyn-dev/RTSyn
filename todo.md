[test] Comedi plugin
[ ] Impove, change and refactor the plotter: It shall generate plots from different inputs, this should be the y value and the x the time (period of collection)
[ ] Allow changing the color of the plotter lines
[ ] When confirmation dialog is shown, the connections on background dissapear visually while is shown.
[ ] When you right click on empty space (not plugin or window), a menu with Add plugin must appear
[ ] Fix warning `warning: the following packages contain code that will be rejected by a future version of Rust: ashpd v0.8.1`
[ ] Click to highlight the connections must be ON the plugin (right now, if you click a window over the plugin they highlight). Also if you click a connection on add connection menu or remove connection menu, it must be highlighted (and follow the same rules as highlighting on click on plugin)
[ ] If a plugin is deleted, reuse id
[ ] Ensure cores are being used
[ ] Windows must be on top of plugins and connection hover info always (but not over the tabs on top (Workspace Plugins ...))
[ ] New workspace should load the new empty workspace after creation
[ ] Fix spaceing of letters in descriptions
[ ] Close "no hooked windows". This is basically that if I have a Plugin config window opened and I change workspaces, remove the plugin etc, it should dissapear
[ ] export workspace file dialog should have the default name of the workspace
[ ] Notify the loading of workspace "Workspace 'name' loaded"
[ ] Notify when period frequency are applied. Name it "Time scale" and as description put "Sampling rate updated"
[ ] Make workspaces on Workspace load and workspace manage list bigger and a bit separated
[ ] Refactor plugin to better programming of the gui
[ ] Python library that is compiled to c (numba?)
[ ] Loading wheel to the right of the text
[ ] Right click over a plugin with a window over it shows the plugin right click menu (it shouldn't)
[ ] Add different connections (shared memory and pipes)
[ ] Python plugin development
[ ] Performance tests
[ ] Hover connections are shown on top of other windows
[ ] Let the installed plugins be organized in folders, there is one reserved for the System ones
[ ] When period is 100 us and window 10\*3500 plot isn't complete and the more I increment the window the less is shown (max points draw related, I think)
[ ] max_latency_us should be max_latency and have a input similar of the settings (the value and the units)
[ ] Eliminate dot for notifications
[ ] Deleting a plugin closes the window and references from it. Reinstalling it closes them and reopens them (with same connections)
