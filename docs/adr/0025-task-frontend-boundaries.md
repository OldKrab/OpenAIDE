# Task Frontend Boundaries

Status: accepted

This ADR defines the Frontend and App Server module boundaries supporting the accepted [Task Lifecycle and Chat Specification](../task-chat-flow.md). Concrete private type names may vary when they preserve these ownership boundaries.

## Composition Boundary

The shared Frontend composition root wires cohesive workflow owners and exposes render-ready state plus user-intent operations. It does not own workflow state machines or expose raw reducer dispatch, request generations, shell routes, or transport details to rendering components.

The required responsibilities are represented by this conceptual interface. Private names may vary; ownership and call direction do not.

```text
ShellRouter
  currentRoute()
  navigate(route)
  subscribe(listener)

NewTaskController
  state()
  open()
  create(selection)
  setConfigOption(configId, value)
  send(message)
  discard()

TaskDraftStore
  get(taskId)
  updateText(taskId, text)
  addAttachment(taskId, row)
  removeAttachment(taskId, rowId)
  markSending(taskId, attempt)
  reconcileSend(taskId, result)
  clear(taskId)

NativeSessionService
  acquire(agentId, projectContext)
  startPrompt(handle, message)
  steer(handle, message)
  setConfigOption(handle, configId, value)
  close(handle)
```

`NewTaskController` owns the New Task state machine and consumes App Server baselines and events. `TaskDraftStore` owns local unsent Composer state keyed by real Task id. `ShellRouter` receives shell-neutral typed routes; browser URLs and VS Code panels remain in their App Shell adapters.

`NativeSessionService` is a deep App Server module. `acquire` returns an opaque handle with one Native Session update consumer already connected to Task projection. `startPrompt` uses that handle and internally returns an existing live ACP session or performs required load, resume, or recreation. Send callers never inspect readiness fields or choose an ACP recovery method. `steer` forwards additional User messages without making their responses part of Task status. The update consumer persists Agent text, Thoughts, Tool activity, transient requests, terminal state, title, options, and commands for the handle lifetime.

## New Task Presentation State

Frontend represents New Task presentation as one exclusive phase rather than parallel booleans:

```text
absent
creating
preparing(taskId)
ready(taskId)
sending(taskId)
connectionLost(taskId)
failed(taskId?, recoverableError)
```

These phases are presentation state derived from authoritative Task state, connection state, and the one local Send attempt. They do not create a second Task lifecycle model.
