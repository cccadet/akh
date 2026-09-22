import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type Project = { name: string; path: string; repositoryUrl: string | null; synced: boolean };
type Task = { id: number; title: string; project: string; branch: string; latestCommit: string | null; lastAgent: string | null; synced: boolean };
type State = { serverConfigured: boolean; worktreeRoot: string; terminalCommand: string; projects: Project[]; tasks: Task[] };
type BackupResult = { path: string; projects: number; tasks: number; conversations: number; databaseIncluded: boolean; restartRequired: boolean };

function App() {
  const [state, setState] = React.useState<State | null>(null);
  const [error, setError] = React.useState("");
  const [path, setPath] = React.useState("");
  const [name, setName] = React.useState("");
  const [taskTitle, setTaskTitle] = React.useState("");
  const [taskProject, setTaskProject] = React.useState("");
  const [taskBranch, setTaskBranch] = React.useState("");
  const [notice, setNotice] = React.useState("");
  const refresh = React.useCallback(() => invoke<State>("get_state").then(setState).catch((e) => setError(String(e))), []);
  React.useEffect(() => { void refresh(); }, [refresh]);

  async function linkProject(event: React.FormEvent) {
    event.preventDefault(); setError("");
    try { await invoke("link_project", { path, name: name || null }); setPath(""); setName(""); await refresh(); }
    catch (e) { setError(String(e)); }
  }

  async function launch(id: number, agent: string) {
    setError("");
    try { await invoke("launch_task", { id, agent }); }
    catch (e) { setError(String(e)); }
  }

  async function createTask(event: React.FormEvent) {
    event.preventDefault(); setError("");
    const project = taskProject || state?.projects[0]?.name;
    if (!project) { setError("Link a project before creating a task."); return; }
    try {
      await invoke<number>("create_task", { project, title: taskTitle, branch: taskBranch || null });
      setTaskTitle(""); setTaskBranch(""); await refresh();
    } catch (e) { setError(String(e)); }
  }

  async function handoff(id: number) {
    const to = window.prompt("Recipient user UUID"); if (!to) return;
    const note = window.prompt("Handoff note") ?? "";
    try { await invoke("handoff_task", { id, to, note }); }
    catch (e) { setError(String(e)); }
  }

  async function runBackup(action: "export_backup" | "import_backup") {
    setError(""); setNotice("");
    try {
      const result = await invoke<BackupResult | null>(action);
      if (!result) return;
      const verb = action === "export_backup" ? "Exported" : "Imported";
      const restart = result.restartRequired ? " Restart Akh to activate the imported database." : "";
      setNotice(`${verb} ${result.projects} projects, ${result.tasks} tasks and ${result.conversations} conversations. File: ${result.path}.${restart}`);
      await refresh();
    } catch (e) { setError(String(e)); }
  }

  if (!state) return <main className="loading">Loading Akh…</main>;
  return <div className="shell">
    <aside><div className="brand">AKH<span>Task continuity</span></div><nav><a href="#home">Home</a><a href="#projects">Projects</a><a href="#tasks">Tasks</a><a href="#settings">Settings</a></nav></aside>
    <main>
      <header id="home"><div><p className="eyebrow">LOCAL CONTROL PLANE</p><h1>Continue where the work stopped.</h1></div><span className={state.serverConfigured ? "status online" : "status"}>{state.serverConfigured ? "Server linked" : "Local only"}</span></header>
      {error && <div className="error">{error}</div>}
      {notice && <div className="notice">{notice}</div>}
      <section id="projects"><div className="section-title"><h2>Projects</h2><span>{state.projects.length}</span></div>
        <form onSubmit={linkProject} className="link-form"><input aria-label="Project name" placeholder="Name (optional)" value={name} onChange={(e) => setName(e.target.value)} /><input required aria-label="Repository path" placeholder="Local repository path" value={path} onChange={(e) => setPath(e.target.value)} /><button>Link repository</button></form>
        <div className="grid">{state.projects.map((project) => <article key={project.name}><div className="project-mark">{project.name.slice(0,2).toUpperCase()}</div><div><h3>{project.name}</h3><p>{project.path}</p><small>{project.repositoryUrl ?? "No origin"}</small></div><span className={project.synced ? "pill synced" : "pill"}>{project.synced ? "synced" : "local"}</span></article>)}</div>
      </section>
      <section id="tasks"><div className="section-title"><h2>Tasks</h2><span>{state.tasks.length}</span></div>
        <form onSubmit={createTask} className="task-form">
          <select required aria-label="Task project" value={taskProject} onChange={(e) => setTaskProject(e.target.value)} disabled={!state.projects.length}>
            <option value="">{state.projects.length ? "Select project" : "Link a project first"}</option>
            {state.projects.map((project) => <option key={project.name} value={project.name}>{project.name}</option>)}
          </select>
          <input required aria-label="Task title" placeholder="Task title" value={taskTitle} onChange={(e) => setTaskTitle(e.target.value)} />
          <input aria-label="Task branch" placeholder="Branch (automatic: task/ID)" value={taskBranch} onChange={(e) => setTaskBranch(e.target.value)} />
          <button disabled={!state.projects.length}>Create task</button>
        </form>
        <div className="tasks">{state.tasks.map((task) => <article key={task.id}><div className="task-id">#{task.id}</div><div className="task-copy"><h3>{task.title}</h3><p>{task.project} · {task.branch} · {task.lastAgent ?? "no agent yet"}</p><small>{task.synced ? "Shared" : "Local"} · {task.latestCommit?.slice(0,10) ?? "Not started"}</small></div><div className="actions"><button onClick={() => handoff(task.id)}>Handoff</button><button onClick={() => launch(task.id,"claude")}>Claude</button><button className="primary" onClick={() => launch(task.id,"codex")}>Codex</button></div></article>)}</div>
      </section>
      <section id="settings"><div className="section-title"><h2>Settings</h2></div><div className="settings"><label>Worktrees<strong>{state.worktreeRoot}</strong></label><label>Terminal<strong>{state.terminalCommand}</strong></label></div><div className="backup-actions"><div><h3>Portable backup</h3><p>Exports tasks, conversations, profiles and SQLite without tokens, worktrees or local repository paths.</p></div><button onClick={() => runBackup("import_backup")}>Import backup</button><button className="primary" onClick={() => runBackup("export_backup")}>Export backup</button></div></section>
    </main>
  </div>;
}

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
