# Subagent History Navigation

Status: accepted

Users enter Subagent History either from a Task-header Subagents action or from a Subagent lifecycle row in Task Chat. Every history view has a persistent Agent navigator: Main Agent is always a one-click return to Task Chat and its Composer, while the selected Subagent control opens a compact nested selector for direct switching to any other Subagent without returning through the parent. The selector preserves parent-child hierarchy and spawn order; OpenAIDE does not use permanent child tabs, browser history, or Task Navigation for this relationship. The same navigation model applies at every viewport width and supports equivalent keyboard navigation.

Navigation is always deliberate: a newly created or terminal Subagent never steals selection, and completing the inspected Subagent leaves its history visible with updated status. Selection is ephemeral Frontend state retained only while the current Task remains mounted; opening a Task later defaults to the Main Agent rather than restoring an old observational view.

The Agent navigator retains every Subagent in spawn order and preserves nesting. Branches are collapsible with ephemeral collapse state, branches containing running or failed descendants start expanded, and the tree scrolls instead of hiding completed history; search and filtering remain deferred until usage demonstrates a need. Main Agent and Subagent entries show current lifecycle, explicit failure, and a restrained per-client new-activity indicator that clears when that history is selected and never becomes durable Task unread state.

Tasks with no Subagents retain the existing Main Agent Chat without empty navigation chrome. The navigator and Task-header Subagents action appear when the first native Subagent is recorded and remain available with its durable history.
