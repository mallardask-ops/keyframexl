# BMS application pass

Source: supplied BMS tokens/tokens.json, DESIGN.md, assets/fonts and Qt/web component examples. IBM Plex Sans for controls; Plex Mono for transport and step positions. Base #141516, surface #1A1B1D, raised #222326, text #E9EAEB, accent #F0584C; control fills and semantic colors use the corresponding BMS tokens.

The Rack is the main surface, spanning the available width with four separated beat groups. Left-align instrument names, align every instrument against the same time columns, and center the transport buttons within their functional groups. Put Demo Bass on the same visual level as drums, with a note preview and a direct editor action. Retain keyboard focus and distinct hover/selected/playing states.

Adapted from the EP–133 guide: stable groups of related controls and immediately visible mode labels. Four groups here mean four beats, not hardware sound banks. No replicated keypad, display or secret shift combinations. A filled red step means an event; a moving white frame and caret mean playback position. Motion is restricted to that position and meters.

Piano Roll shares transport, pattern selection, type and palette. The slide trajectory carries the visual emphasis. One chromatic row per MIDI key; key labels and notes use the same coordinate function. Fit the C3–C5 span within a 1366×768 viewport, with a separate velocity lane below it.

Review against brief: removed the small default button strip and isolated bass launcher. Avoid decorative hardware screws, giant numbers or an ornamental grid. The only grid represents musical time/pitch. ENERGY 2 / RHYTHM 2 / MOTION 1.
