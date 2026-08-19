# File Lines Become Composer Quotes

Status: accepted

The File Viewer line action inserts a File Quote into the ordinary Composer rather than creating a persisted comment entity or sending immediately. The inserted text is an ordinary Chat-style `>` Quote: the first quoted line is the File Viewer's display path plus `:line`, the second is the captured line. It remains fully editable and has no durable relationship to the source file after insertion. The action is a small gutter button to the left of the line. One activation inserts the File Quote immediately. The button appears on hover and keyboard focus; on touch or a narrow Task Page a compact control remains available without hover. It is not a Chat-style free selection Quote. This reuses the existing Quote and Composer model, while deliberately leaving threaded comments, live line anchors, comment resolution, and multi-line or range File Quotes for a separate feature.
