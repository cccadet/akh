import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type Project = { name: string; path: string; repositoryUrl: string | null; synced: boolean };
type Message = { role: string; agent: string; content: string; createdAt: number };
type Handoff = { id: string; fromUserId: string; note: string };
type Task = { id: number; title: string; project: string; branch: string; latestCommit: string | null; lastAgent: string | null; synced: boolean; dirty: boolean; commitPushed: boolean | null; messages: Message[]; pendingHandoffs: Handoff[] };
type Profile = { name: string; command: string; args: string[] };
type User = { id: string; username: string; email: string };
type Notification = { id: string; taskId: number | null; kind: string; content: string; createdAt: string };
type State = { serverConfigured: boolean; server: string | null; worktreeRoot: string; terminalCommand: string; projects: Project[]; tasks: Task[]; profiles: Profile[]; users: User[]; notifications: Notification[] };
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
  const [terminalCommand, setTerminalCommand] = React.useState("");
  const [worktreeRoot, setWorktreeRoot] = React.useState("");
  const [profileName, setProfileName] = React.useState("");
  const [profileCommand, setProfileCommand] = React.useState("");
  const [profileArgs, setProfileArgs] = React.useState("");
  const [serverUrl, setServerUrl] = React.useState("http://127.0.0.1:3000");
  const [identity, setIdentity] = React.useState("");
  const [email, setEmail] = React.useState("");
  const [username, setUsername] = React.useState("");
  const [password, setPassword] = React.useState("");
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
    const choices = state?.users.map((user) => `${user.username}: ${user.id}`).join("\n") ?? "";
    const to = window.prompt(`Recipient user UUID\n\n${choices}`); if (!to) return;
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

  async function saveTerminal(event: React.FormEvent) {
    event.preventDefault(); setError("");
    const parts = terminalCommand.trim().split(/\s+/);
    try { await invoke("save_terminal", { command: parts[0], args: parts.slice(1) }); setNotice("Terminal configuration saved."); await refresh(); }
    catch (e) { setError(String(e)); }
  }

  async function saveWorktree(event: React.FormEvent) {
    event.preventDefault(); setError("");
    try { await invoke("save_worktree_root", { path: worktreeRoot }); setWorktreeRoot(""); setNotice("Worktree directory saved."); await refresh(); }
    catch (e) { setError(String(e)); }
  }

  async function saveProfile(event: React.FormEvent) {
    event.preventDefault(); setError("");
    try { await invoke("save_profile", { name: profileName, command: profileCommand, args: profileArgs.trim() ? profileArgs.trim().split(/\s+/) : [] }); setProfileName(""); setProfileCommand(""); setProfileArgs(""); setNotice("Agent profile saved."); await refresh(); }
    catch (e) { setError(String(e)); }
  }

  async function authenticate(mode: "login_server" | "register_server") {
    setError("");
    try {
      const input = mode === "login_server" ? { server: serverUrl, identity, password } : { server: serverUrl, email, username, password };
      await invoke(mode, input); setPassword(""); setNotice(mode === "login_server" ? "Logged in." : "Account created and logged in."); await refresh();
    } catch (e) { setError(String(e)); }
  }

  async function synchronize() {
    setError("");
    try { const output = await invoke<string>("sync_now"); setNotice(output || "Synchronization complete."); await refresh(); }
    catch (e) { setError(String(e)); }
  }

  async function logout() {
    try { await invoke("logout_server"); setNotice("Logged out."); await refresh(); }
    catch (e) { setError(String(e)); }
  }

  async function acceptHandoff(id: string) {
    try { await invoke("accept_handoff", { id }); setNotice("Handoff accepted."); await synchronize(); }
    catch (e) { setError(String(e)); }
  }

  if (!state) return <main className="loading">Loading Akh…</main>;
  return <div className="shell">
    <aside><div className="brand">AKH<span>Task continuity</span></div><nav><a href="#home">Home</a><a href="#projects">Projects</a><a href="#tasks">Tasks</a><a href="#settings">Settings</a></nav></aside>
    <main>
      <header id="home"><div><p className="eyebrow">LOCAL CONTROL PLANE</p><h1>Continue where the work stopped.</h1></div><span className={state.serverConfigured ? "status online" : "status"}>{state.serverConfigured ? "Server linked" : "Local only"}</span></header>
      {error && <div className="error">{error}</div>}
      {notice && <div className="notice">{notice}</div>}
      {state.notifications.length > 0 && <section className="notifications"><div className="section-title"><h2>Notifications</h2><span>{state.notifications.length}</span></div>{state.notifications.map((item) => <article key={item.id}><strong>{item.kind}</strong><span>{item.content}</span><small>{new Date(item.createdAt).toLocaleString()}</small></article>)}</section>}
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
        <div className="tasks">{state.tasks.map((task) => <article key={task.id}><div className="task-row"><div className="task-id">#{task.id}</div><div className="task-copy"><h3>{task.title}</h3><p>{task.project} · {task.branch} · {task.lastAgent ?? "no agent yet"}</p><small>{task.synced ? "Conversation shared" : "Local only"} · {task.latestCommit?.slice(0,10) ?? "Not started"} · {task.dirty ? "⚠ uncommitted changes" : task.commitPushed === true ? "✓ commit pushed" : task.commitPushed === false ? "⚠ commit not pushed" : "no shared commit"}</small></div><div className="actions"><button onClick={() => handoff(task.id)}>Handoff</button>{state.profiles.map((profile) => <button className={profile.name === "codex" ? "primary" : ""} key={profile.name} onClick={() => launch(task.id,profile.name)}>{profile.name}</button>)}</div></div>{task.pendingHandoffs.map((handoff) => <div className="handoff" key={handoff.id}><span>Handoff from {handoff.fromUserId}: {handoff.note}</span><button onClick={() => acceptHandoff(handoff.id)}>Accept</button></div>)}{task.messages.length > 0 && <details><summary>Conversation · {task.messages.length} messages</summary><div className="conversation">{task.messages.map((message, index) => <div key={`${message.createdAt}-${index}`}><small>{message.agent} · {message.role}</small><p>{message.content}</p></div>)}</div></details>}</article>)}</div>
      </section>
      <section id="settings"><div className="section-title"><h2>Settings</h2></div><div className="settings"><label>Worktrees<strong>{state.worktreeRoot}</strong></label><label>Terminal<strong>{state.terminalCommand}</strong></label></div>
        <div className="auth-panel"><div><h3>Server and authentication</h3><p>{state.serverConfigured ? `Connected to ${state.server}` : "Use the embedded local server or a team server."}</p></div><input placeholder="Server URL" value={serverUrl} onChange={(e) => setServerUrl(e.target.value)} /><input placeholder="Email or username" value={identity} onChange={(e) => setIdentity(e.target.value)} /><input type="password" placeholder="Password" value={password} onChange={(e) => setPassword(e.target.value)} /><button onClick={() => authenticate("login_server")}>Login</button><button onClick={synchronize} disabled={!state.serverConfigured}>Sync now</button>{state.serverConfigured && <button onClick={logout}>Logout</button>}</div>
        {!state.serverConfigured && <div className="auth-panel register-panel"><div><h3>Create account</h3><p>Authentication is local when using the embedded server.</p></div><input type="email" placeholder="Email" value={email} onChange={(e) => setEmail(e.target.value)} /><input placeholder="Username" value={username} onChange={(e) => setUsername(e.target.value)} /><button onClick={() => authenticate("register_server")}>Register</button></div>}
        <form className="settings-form" onSubmit={saveTerminal}><div><h3>System terminal</h3><p>Command followed by fixed arguments.</p></div><input required placeholder="wt.exe" value={terminalCommand} onChange={(e) => setTerminalCommand(e.target.value)} /><button>Save terminal</button></form>
        <form className="settings-form" onSubmit={saveWorktree}><div><h3>Worktree directory</h3><p>Local only; never sent to the server.</p></div><input required placeholder={state.worktreeRoot} value={worktreeRoot} onChange={(e) => setWorktreeRoot(e.target.value)} /><button>Save directory</button></form>
        <form className="settings-form profile-form" onSubmit={saveProfile}><div><h3>Agent profile</h3><p>Use commands such as codex, claude or headroom.</p></div><input required placeholder="Profile name" value={profileName} onChange={(e) => setProfileName(e.target.value)} /><input required placeholder="Command" value={profileCommand} onChange={(e) => setProfileCommand(e.target.value)} /><input placeholder="Arguments" value={profileArgs} onChange={(e) => setProfileArgs(e.target.value)} /><button>Save profile</button></form>
        <div className="profile-list">{state.profiles.map((profile) => <span key={profile.name}><strong>{profile.name}</strong> {profile.command} {profile.args.join(" ")}</span>)}</div>
        <div className="backup-actions"><div><h3>Portable backup</h3><p>Exports tasks, conversations, profiles and SQLite without tokens, worktrees or local repository paths.</p></div><button onClick={() => runBackup("import_backup")}>Import backup</button><button className="primary" onClick={() => runBackup("export_backup")}>Export backup</button></div></section>
    </main>
  </div>;
}

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
