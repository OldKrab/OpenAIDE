# File Viewer Task Page Layout

Status: superseded by [ADR-0066](0066-project-files-belong-to-a-workspace-and-open-beside-chat.md)

Project files is a Task Page destination that replaces the former Chat/Task Panel split. The header entry and Agent File References open the same navigator and File Viewer. Wide screens retain the resizable navigator beside the reader; narrow screens move between list and reader with a local back action. Back to conversation returns to mounted Chat and its unchanged draft. There are no separate hide-sidebar or show-chat toggles. Closing the last File Tab leaves the project navigator and an empty reader available. File/Diff switches preserve tabs and source metrics. See the accepted [production integration plan](../plans/project-file-access.md).

The full-page destination hid Chat, so each comment cost a round trip between code and conversation. Project files now opens as a panel beside Chat, resizable and maximizable, with a separate full page reached from a Project.
