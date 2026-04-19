use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{Result, anyhow, bail};
use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskPlan {
    pub id: String,
    pub root_task_id: String,
    #[serde(default)]
    pub tasks: Vec<TaskSpec>,
    #[serde(default)]
    pub dependencies: Vec<TaskDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSpec {
    pub id: String,
    pub agent_service: String,
    pub prompt: String,
    #[serde(default)]
    pub input: Value,
    pub output_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDependency {
    pub from_task: String,
    pub to_task: String,
    #[serde(default)]
    pub bindings: Vec<OutputBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputBinding {
    pub from_pointer: String,
    pub to_pointer: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    WaitingChildPlan,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildPlanLink {
    pub parent_plan_id: String,
    pub parent_task_id: String,
    pub child_plan_id: String,
}

pub trait TaskPlanStore {}

pub trait AgentDirectory {}

pub trait AgentInvoker {}

pub fn validate_plan(plan: &TaskPlan) -> Result<()> {
    validate_non_empty(&plan.id, "plan.id")?;
    validate_non_empty(&plan.root_task_id, "plan.root_task_id")?;
    if plan.tasks.is_empty() {
        bail!("task plan must contain at least one task");
    }

    let mut task_ids = BTreeSet::new();
    for task in &plan.tasks {
        validate_non_empty(&task.id, "task.id")?;
        validate_non_empty(&task.agent_service, "task.agent_service")?;
        validate_non_empty(&task.prompt, "task.prompt")?;
        if !task_ids.insert(task.id.clone()) {
            bail!("duplicate task id `{}`", task.id);
        }
        validate_json_schema(&task.output_schema)
            .map_err(|error| anyhow!("task `{}` output_schema is invalid: {error}", task.id))?;
    }

    if !task_ids.contains(&plan.root_task_id) {
        bail!(
            "root_task_id `{}` does not reference a task",
            plan.root_task_id
        );
    }

    for dependency in &plan.dependencies {
        validate_non_empty(&dependency.from_task, "dependency.from_task")?;
        validate_non_empty(&dependency.to_task, "dependency.to_task")?;
        if dependency.from_task == dependency.to_task {
            bail!("task `{}` cannot depend on itself", dependency.from_task);
        }
        if !task_ids.contains(&dependency.from_task) {
            bail!(
                "dependency from_task `{}` does not reference a task",
                dependency.from_task
            );
        }
        if !task_ids.contains(&dependency.to_task) {
            bail!(
                "dependency to_task `{}` does not reference a task",
                dependency.to_task
            );
        }
        for binding in &dependency.bindings {
            validate_json_pointer(&binding.from_pointer, "binding.from_pointer")?;
            validate_json_pointer(&binding.to_pointer, "binding.to_pointer")?;
        }
    }

    validate_acyclic(plan, &task_ids)
}

pub fn ready_task_ids(
    plan: &TaskPlan,
    states: &BTreeMap<String, TaskState>,
) -> Result<Vec<String>> {
    validate_plan(plan)?;
    let mut ready = Vec::new();
    for task in &plan.tasks {
        let state = states.get(&task.id).copied().unwrap_or(TaskState::Queued);
        if state != TaskState::Queued {
            continue;
        }
        let dependencies_succeeded = plan
            .dependencies
            .iter()
            .filter(|dependency| dependency.to_task == task.id)
            .all(|dependency| {
                states.get(&dependency.from_task).copied() == Some(TaskState::Succeeded)
            });
        if dependencies_succeeded {
            ready.push(task.id.clone());
        }
    }
    Ok(ready)
}

pub fn apply_output_bindings(
    plan: &TaskPlan,
    task_id: &str,
    base_input: &Value,
    outputs: &BTreeMap<String, Value>,
) -> Result<Value> {
    validate_plan(plan)?;
    if !plan.tasks.iter().any(|task| task.id == task_id) {
        bail!("task `{task_id}` does not exist in plan `{}`", plan.id);
    }

    let mut input = base_input.clone();
    for dependency in plan
        .dependencies
        .iter()
        .filter(|dependency| dependency.to_task == task_id)
    {
        let output = outputs
            .get(&dependency.from_task)
            .ok_or_else(|| anyhow!("missing output for dependency `{}`", dependency.from_task))?;
        for binding in &dependency.bindings {
            let value = output.pointer(&binding.from_pointer).ok_or_else(|| {
                anyhow!(
                    "output pointer `{}` not found in task `{}` output",
                    binding.from_pointer,
                    dependency.from_task
                )
            })?;
            set_json_pointer(&mut input, &binding.to_pointer, value.clone())?;
        }
    }
    Ok(input)
}

pub fn validate_task_output(task: &TaskSpec, output: &Value) -> Result<()> {
    let compiled =
        JSONSchema::compile(&task.output_schema).map_err(|error| anyhow!(error.to_string()))?;
    match compiled.validate(output) {
        Ok(()) => Ok(()),
        Err(errors) => {
            let messages = errors
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            bail!("{messages}")
        }
    }
}

fn validate_acyclic(plan: &TaskPlan, task_ids: &BTreeSet<String>) -> Result<()> {
    let mut outgoing = task_ids
        .iter()
        .map(|id| (id.clone(), Vec::<String>::new()))
        .collect::<BTreeMap<_, _>>();
    let mut indegree = task_ids
        .iter()
        .map(|id| (id.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();

    for dependency in &plan.dependencies {
        outgoing
            .get_mut(&dependency.from_task)
            .ok_or_else(|| anyhow!("dependency references missing task"))?
            .push(dependency.to_task.clone());
        *indegree
            .get_mut(&dependency.to_task)
            .ok_or_else(|| anyhow!("dependency references missing task"))? += 1;
    }

    let mut queue = indegree
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<VecDeque<_>>();
    let mut visited = 0usize;

    while let Some(task_id) = queue.pop_front() {
        visited += 1;
        for to_task in outgoing.get(&task_id).into_iter().flatten() {
            let count = indegree
                .get_mut(to_task)
                .ok_or_else(|| anyhow!("dependency references missing task"))?;
            *count -= 1;
            if *count == 0 {
                queue.push_back(to_task.clone());
            }
        }
    }

    if visited == task_ids.len() {
        Ok(())
    } else {
        bail!("task plan contains a dependency cycle")
    }
}

fn validate_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} cannot be empty");
    }
    Ok(())
}

fn validate_json_schema(schema: &Value) -> Result<()> {
    JSONSchema::compile(schema)
        .map(|_| ())
        .map_err(|error| anyhow!(error.to_string()))
}

fn validate_json_pointer(pointer: &str, field: &str) -> Result<()> {
    if pointer.is_empty() || pointer.starts_with('/') {
        Ok(())
    } else {
        bail!("{field} must be an RFC 6901 JSON pointer")
    }
}

fn set_json_pointer(target: &mut Value, pointer: &str, value: Value) -> Result<()> {
    validate_json_pointer(pointer, "to_pointer")?;
    if pointer.is_empty() {
        *target = value;
        return Ok(());
    }

    let tokens = pointer
        .split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .collect::<Result<Vec<_>>>()?;
    if tokens.is_empty() {
        *target = value;
        return Ok(());
    }

    let mut current = target;
    for token in &tokens[..tokens.len() - 1] {
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
        let object = current
            .as_object_mut()
            .ok_or_else(|| anyhow!("to_pointer can only create object paths"))?;
        current = object
            .entry(token.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }

    if !current.is_object() {
        *current = Value::Object(Map::new());
    }
    let object = current
        .as_object_mut()
        .ok_or_else(|| anyhow!("to_pointer can only set object fields"))?;
    let last = tokens
        .last()
        .ok_or_else(|| anyhow!("to_pointer cannot be empty here"))?;
    object.insert(last.clone(), value);
    Ok(())
}

fn unescape_pointer_token(token: &str) -> Result<String> {
    let mut output = String::new();
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '~' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('0') => output.push('~'),
            Some('1') => output.push('/'),
            Some(other) => bail!("invalid JSON pointer escape `~{other}`"),
            None => bail!("invalid trailing JSON pointer escape"),
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_plan() -> TaskPlan {
        TaskPlan {
            id: "plan_1".to_owned(),
            root_task_id: "research".to_owned(),
            tasks: vec![
                TaskSpec {
                    id: "research".to_owned(),
                    agent_service: "research-agent".to_owned(),
                    prompt: "Research competitors".to_owned(),
                    input: json!({ "topic": "devtools" }),
                    output_schema: json!({
                        "type": "object",
                        "required": ["findings"],
                        "properties": {
                            "findings": { "type": "array" }
                        }
                    }),
                },
                TaskSpec {
                    id: "synthesis".to_owned(),
                    agent_service: "strategy-agent".to_owned(),
                    prompt: "Synthesize findings".to_owned(),
                    input: json!({}),
                    output_schema: json!({ "type": "object" }),
                },
            ],
            dependencies: vec![TaskDependency {
                from_task: "research".to_owned(),
                to_task: "synthesis".to_owned(),
                bindings: vec![OutputBinding {
                    from_pointer: "/findings".to_owned(),
                    to_pointer: "/research_findings".to_owned(),
                }],
            }],
        }
    }

    #[test]
    fn validates_sample_plan() {
        validate_plan(&sample_plan()).unwrap();
    }

    #[test]
    fn rejects_dependency_cycle() {
        let mut plan = sample_plan();
        plan.dependencies.push(TaskDependency {
            from_task: "synthesis".to_owned(),
            to_task: "research".to_owned(),
            bindings: Vec::new(),
        });

        let error = validate_plan(&plan).unwrap_err().to_string();
        assert!(error.contains("cycle"));
    }

    #[test]
    fn computes_ready_tasks_from_succeeded_dependencies() {
        let plan = sample_plan();
        let mut states = BTreeMap::new();
        assert_eq!(ready_task_ids(&plan, &states).unwrap(), vec!["research"]);

        states.insert("research".to_owned(), TaskState::Succeeded);
        assert_eq!(ready_task_ids(&plan, &states).unwrap(), vec!["synthesis"]);
    }

    #[test]
    fn applies_output_bindings() {
        let plan = sample_plan();
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "research".to_owned(),
            json!({ "findings": [{ "name": "A" }] }),
        );

        let input = apply_output_bindings(&plan, "synthesis", &json!({}), &outputs).unwrap();
        assert_eq!(input, json!({ "research_findings": [{ "name": "A" }] }));
    }

    #[test]
    fn validates_task_output_against_schema() {
        let plan = sample_plan();
        let task = &plan.tasks[0];
        validate_task_output(task, &json!({ "findings": [] })).unwrap();
        assert!(validate_task_output(task, &json!({ "summary": "missing" })).is_err());
    }
}
