use super::*;

impl NativeSessionService {
    pub(super) fn prepare_task_inner(
        &self,
        task: &TaskRecord,
        cancellation: TurnCancellation,
    ) -> Result<(), RuntimeError> {
        let (session, missing_session_id) = match &task.agent_session_id {
            Some(session_id) => match self.agent_gateway.resume_session(AgentSessionResume {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                session_id: session_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                cancellation: cancellation.clone(),
                secret_resolver: Some(self.secret_resolver(&task.task_id)),
            }) {
                Ok(session) => (session, None),
                Err(error) if is_session_resume_unsupported(&error) => {
                    match self.agent_gateway.load_session(AgentSessionLoad {
                        agent_id: task.agent_id.clone(),
                        task_id: task.task_id.clone(),
                        session_id: session_id.clone(),
                        cwd: task.workspace_root.clone(),
                        model_id: task.model_id.clone(),
                        cancellation: cancellation.clone(),
                        secret_resolver: Some(self.secret_resolver(&task.task_id)),
                    }) {
                        Ok(loaded) => (loaded.session, None),
                        Err(
                            RuntimeError::TaskNotFound(_) | RuntimeError::NativeSessionMissing(_),
                        ) => (
                            self.start_new_session(task, cancellation.clone())?,
                            Some(session_id.clone()),
                        ),
                        Err(error) => return Err(error),
                    }
                }
                Err(RuntimeError::TaskNotFound(_) | RuntimeError::NativeSessionMissing(_)) => (
                    self.start_new_session(task, cancellation.clone())?,
                    Some(session_id.clone()),
                ),
                Err(error) => return Err(error),
            },
            None => (self.start_new_session(task, cancellation.clone())?, None),
        };
        let session_start = TaskSessionStartGuard::new(&self.agent_gateway, session);
        let new_session = task.agent_session_id.is_none() || missing_session_id.is_some();
        let preferences = if new_session {
            Some(self.store.read_agent_config_preferences(&task.agent_id)?)
        } else {
            None
        };
        let preference_state = preferences.as_ref().and_then(|preferences| {
            crate::tasks::config_preferences::initial_state(
                preferences,
                session_start.session().config_catalog.as_ref(),
            )
        });
        let _ownership = PreparingSessionOwnership::reserve(
            self.preparing_session_ids.clone(),
            session_start.session().key(),
        )?;
        let session_id = session_start.session().session_id.clone();
        let config_catalog = session_start.session().config_catalog.clone();
        let commands_catalog = session_start.session().commands_catalog.clone();
        let model_id = session_start.session().model_id.clone();
        let supports_image_input = session_start.session().prompt_capabilities.image;
        let prompt_capabilities_authoritative =
            session_start.session().prompt_capabilities_authoritative;
        let replacement_metadata_is_authoritative = missing_session_id.is_some();
        let now = now_string();

        let bind_session = |ctx: &mut crate::tasks::mutation::TaskMutationContext<'_>| {
            if ctx.task().tombstoned
                || ctx.task().agent_session_id != task.agent_session_id
                || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
            {
                return Ok(TaskMutationResult::Rejected);
            }
            let task = ctx.task_mut();
            task.agent_session_id = Some(session_id.clone());
            task.config_mutation.preferences = preference_state.clone();
            // Resume can return an identity-only session. Preserve the last
            // capability snapshot until ACP supplies authoritative metadata.
            if prompt_capabilities_authoritative {
                task.supports_image_input = supports_image_input;
            }
            if replacement_metadata_is_authoritative {
                task.config_options_catalog = config_catalog.clone();
                task.agent_commands_catalog = commands_catalog.clone();
                task.model_id = model_id.clone();
                // A newly started replacement session defines the complete
                // process-local catalog state, including an authoritative
                // absence of options or commands.
                task.native_session_data_freshness = Default::default();
            } else {
                if config_catalog.is_some() {
                    task.config_options_catalog = config_catalog.clone();
                    task.native_session_data_freshness.mark_config_fresh();
                    task.model_id = model_id.clone();
                }
                if commands_catalog.is_some() {
                    if task.agent_commands_catalog.is_none() {
                        task.agent_commands_catalog = commands_catalog.clone();
                    }
                    task.native_session_data_freshness.mark_commands_fresh();
                }
            }
            task.updated_at = now.clone();
            Ok(TaskMutationResult::Changed)
        };
        let binding = match missing_session_id.as_deref() {
            Some(missing_session_id) => self.mutations.replace_missing_session_for_prepared_task(
                &task.task_id,
                missing_session_id,
                TaskCommitOptions::metadata(),
                bind_session,
            )?,
            None => self.mutations.commit_existing_task(
                &task.task_id,
                TaskCommitOptions::metadata(),
                bind_session,
            )?,
        };
        if !matches!(binding.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(RuntimeError::NotReady(
                "Task changed before Agent preparation completed".to_string(),
            ));
        }
        if missing_session_id.is_some() {
            log_missing_session_replaced(&task.task_id, &task.agent_id, &session_id, "preparation");
        }

        if let Err(error) =
            self.ensure_update_subscription(&task.task_id, &session_start.session().key())
        {
            self.forget_update_subscription(&task.task_id, session_start.session());
            return Err(error);
        }

        // Keep preparation pending while the live catalog is already observable.
        if let Some(preferences) = preferences {
            // Sink attachment can synchronously deliver a newer complete catalog.
            // Re-evaluate before deciding that a saved value is unavailable or already matches.
            self.mutations.commit_existing_task(
                &task.task_id,
                TaskCommitOptions::metadata(),
                |ctx| {
                    let task = ctx.task_mut();
                    if task.tombstoned
                        || task.agent_session_id.as_deref() != Some(&session_id)
                        || !matches!(task.preparation, TaskPreparationRecord::Preparing)
                    {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    let next = crate::tasks::config_preferences::initial_state(
                        &preferences,
                        task.config_options_catalog.as_ref(),
                    );
                    if task.config_mutation.preferences == next {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    task.config_mutation.preferences = next;
                    task.updated_at = now_string();
                    Ok(TaskMutationResult::Changed)
                },
            )?;
            self.apply_initial_preferences(&task.task_id, &session_id, &preferences)?;
        }
        let ready_at = now_string();
        let completion = self.mutations.commit_existing_task(
            &task.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().tombstoned
                    || ctx.task().agent_session_id.as_deref() != Some(session_id.as_str())
                    || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                task.preparation = TaskPreparationRecord::Ready;
                task.updated_at = ready_at;
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if !matches!(completion.outcome, TaskCommitOutcome::Committed(_)) {
            self.forget_update_subscription(&task.task_id, session_start.session());
            return Err(RuntimeError::NotReady(
                "Task changed before Agent preparation completed".to_string(),
            ));
        }
        session_start.commit();
        Ok(())
    }
}
