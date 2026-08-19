# File Viewer Task Page Layout

Status: accepted

On a wide Task Page, Chat and the Task Panel share the page side by side. File Viewer is the v1 occupant of that panel; later inspectable surfaces may use the same column. The user may drag the shared border to resize them; that width is remembered only for this Task Page mount and resets to a default (about 45% Task Panel) on reload or Task switch. One header control collapses and expands the Task Panel in the same place; collapse keeps the current occupant (File Tabs and snapshots in v1) for this Task Page. On a narrow Task Page they do not share horizontally: the Task Panel occupies the full-height page, and that same header control restores Chat rather than a squeezed split or a separate route. Closing the last File Tab dismisses an empty File Viewer so Chat uses the full page; the next open recreates it in the Task Panel.
