# Akh — Plano do Produto

## Visão do produto

**Akh** é uma camada de continuidade de tarefas entre desenvolvedores e coding agents. Não é IDE, agente novo ou terminal: mantém o contexto de uma tarefa enquanto pessoas e agentes, como Claude Code e Codex, entram e saem.

```text
Developer A → Claude Code ┐
                           ├→ Task Akh → conversa + referência Git
Developer B → Codex      ──┘
```

A unidade central é a **Task**:

```text
Task
├── projeto
├── branch e commit compartilhado
├── conversa compartilhada
├── desenvolvedores participantes
└── agentes usados
```

João pode trabalhar na Task #183 com Claude Code, fazer commit e push, e Cristian continuar a mesma task com Codex. O novo agente recebe a conversa relevante e trabalha sobre o mesmo estado Git commitado.

## Princípios

- Agentes executam localmente; o servidor nunca executa Claude, Codex ou outros agentes.
- Git é a fonte da verdade para código, branches, commits, merge, histórico e sincronização.
- Só código commitado e disponibilizado no remoto é compartilhável entre máquinas. Mudanças não commitadas não são sincronizadas.
- Sincronizamos essencialmente entrada da pessoa e resposta do agente, não reasoning interno, tool calls, shell trace ou todos os comandos.
- Worktrees, caminhos locais e preferências de terminal são locais e não vão ao servidor.
- Claude, Codex e OpenCode continuam sendo as ferramentas originais, usadas no terminal do sistema.
- Wrappers como Headroom devem funcionar por comando configurável, sem acoplamento.

## Arquitetura local e team

```text
                         Team Server
              users · projects · tasks · messages
                 agent sessions · Git references
                              │
                        HTTPS / WebSocket
                    ┌─────────┴─────────┐
                    ▼                   ▼
            Máquina de João       Máquina de Cristian
            CLI + Desktop + Core  CLI + Desktop + Core
                    │                   │
            Claude/Codex/etc.     Claude/Codex/etc.
                    │                   │
            Git + worktrees       Git + worktrees
                    └────── Git remote ─┘
```

O Git remote pode ser GitHub, GitLab, Gitea, Forgejo, Azure DevOps ou Bitbucket. Akh não substitui nenhum deles.

### Modo local

Uma pessoa roda servidor, banco, cliente, Git e agentes na mesma máquina, por exemplo com `akh-server` ou Docker Compose. O cliente aponta para localhost. Inicialmente, Postgres no modo local mantém o comportamento igual ao modo team; SQLite pode vir depois.

### Modo team

O servidor fica em um endpoint da empresa, como `https://agents.interno.empresa`. Cada pessoa instala somente o cliente local. Os agentes e repositórios seguem nas máquinas de cada desenvolvedor.

## Componentes

### `akh-core`

Biblioteca Rust compartilhada por CLI e Desktop. Centraliza configuração local, detecção e vínculo de projetos, Git, worktrees, tasks, sincronização, auth, adapters de agentes e lançamento de terminal. CLI e GUI não duplicam lógica de negócio.

### `akh-cli`

Interface rápida no terminal:

```bash
akh login
akh project list
akh project link
akh task list
akh task open 183
akh task 183 claude
akh task 183 codex
```

### `akh-desktop`

GUI local pequena, em Tauri, React e TypeScript. É um painel de configuração, vínculo local, tasks, preferências e launchers; não é editor de código, IDE ou terminal.

### `akh-server`

Servidor central em Rust, Axum, Tokio, SQLx e PostgreSQL. É responsável por auth, usuários, projetos, tasks, conversa, sessões de agente, referências Git e sincronização.

## Identificação e vínculo local de projetos

O servidor armazena uma identidade portátil:

```text
project_id
name
repository_url
default_branch
```

Ele nunca armazena caminhos como `C:\dev\repo` ou `/home/alguem/repo`. Cada cliente mantém seu próprio mapeamento, por exemplo `project 27 → D:\dev\omni-sql`.

Na GUI, um projeto não vinculado mostra o remote e uma ação para selecionar a pasta local. O cliente valida que ela é Git, que o remote corresponde ao projeto, que consegue ler o repositório e executar Git. O vínculo permanece apenas na configuração local.

Dentro de um repositório, a CLI pode descobrir o vínculo automaticamente usando `git rev-parse --show-toplevel` e `git remote get-url origin`.

## Modelo de Task e conversa compartilhada

Exemplo de Task:

```text
Task #183 — Suporte a Oracle Wallet
Project: omni-sql
Branch: feature/oracle-wallet
Latest shared commit: abc123
Status: in_progress
Current developer: Cristian
Last agent: Codex
```

Akh não tenta compartilhar sessões nativas de Claude ou Codex. Ele mantém uma única `TaskConversation`, com mensagens de pessoas e respostas de vários agentes em uma linha do tempo comum.

Cada mensagem começa simples:

```text
id, task_id, user_id, agent, role, content, created_at
```

`agent` identifica Claude ou Codex, por exemplo, e `role` identifica usuário ou assistente. Não há armazenamento de reasoning.

## Git como fonte da verdade

O servidor guarda somente referências: repositório, branch e `commit_sha`. O estado compartilhado da task é o último commit conhecido.

> Estado não commitado é local e não faz parte do handoff.

Quando uma sessão termina, o cliente lê `git rev-parse HEAD` e atualiza o commit da task. Também pode consultar `git status` e deixar a diferença explícita:

```text
Shared state: conversa sincronizada; commit abc123 enviado
Local state: 3 arquivos não commitados — não serão compartilhados
```

## Worktrees locais

Cada Task pode ter worktree próprio, por exemplo `D:\dev\worktrees\omni-sql\183` ou `/home/joao/worktrees/omni-sql/183`. O cliente relaciona task, branch e worktree; o servidor não conhece esses caminhos.

## Fluxo `akh task 183 codex`

Ao executar:

```bash
akh task 183 codex
```

o cliente:

1. consulta a Task e resolve o projeto local;
2. faz `git fetch`;
3. localiza ou cria worktree e faz checkout da branch;
4. verifica o commit compartilhado;
5. recupera a conversa;
6. prepara o contexto de continuidade;
7. inicia Codex no terminal atual.

`akh task 183 claude` usa o mesmo fluxo para Claude Code. Não existe terminal embutido: a TUI nativa aparece no PowerShell, Windows Terminal, WezTerm ou no terminal já usado. Pela GUI, o launcher abre o terminal configurado já no worktree e executa o comando.

## Agent adapters, profiles e Headroom

`akh-core` define um `AgentAdapter`. O primeiro recorte suporta Claude Code e Codex CLI; OpenCode, Gemini e Pi vêm depois. O adapter cuida do lançamento, contexto e captura da conversa.

Os comandos são configuráveis, o que permite Headroom e ferramentas equivalentes sem que Akh precise saber o que fazem:

```toml
[agents.claude]
command = "claude"

[agents.codex]
command = "codex"

[profiles.claude-headroom]
agent = "claude"
command = "headroom"
args = ["wrap", "claude"]
```

Profiles podem ser usados diretamente:

```bash
akh task 183 --profile claude-headroom
```

## Captura apenas de input/output

O objetivo é capturar:

```text
entrada da pessoa → saída do agente
```

Não é necessário capturar tools, reasoning, comandos shell ou eventos internos. Quando um agente oferece hooks, logs ou interfaces estruturadas, eles são preferíveis. Quando não houver, um PTY proxy transparente pode observar stdin/stdout sem criar uma UI própria: entrada e saída permanecem visíveis normalmente no terminal do sistema.

## Context injection

Ao entrar em uma task existente, Akh não tenta reconstruir a sessão nativa do agente. Ele injeta um contexto com projeto, task, branch, commit, conversa e a instrução para inspecionar o repositório e continuar.

```text
You are continuing Task #183.
PROJECT: omni-sql
TASK: Oracle Wallet support
BRANCH: feature/oracle-wallet
CURRENT COMMIT: abc123

SHARED HISTORY
...

CURRENT STATE
Inspect the current repository and continue the task.
```

No MVP, o histórico completo pode ser usado enquanto pequeno. Depois, o padrão é `summary + últimas 10/20 mensagens`, mantendo a conversa completa no servidor.

## Handoff entre desenvolvedores e agentes

No handoff entre pessoas, quem entrega faz commit e push. Quem continua faz fetch, checkout da branch/commit, carrega a conversa e abre seu agente. Entre agentes na mesma máquina, a troca usa o mesmo worktree, branch e conversa com outro profile.

Depois pode haver handoff explícito:

```bash
akh task handoff 183 --to joao
```

Ele inclui destinatário e nota de trabalho; é informação da equipe, não reconstrução de estado interno de agente.

## GUI local

As primeiras telas:

- **Home**: atividade e atalhos.
- **Projects**: projetos do servidor, vínculo local, alterar/remover vínculo, abrir pasta ou terminal.
- **Tasks**: tasks, status, participantes e agente mais recente.
- **Task**: branch, último commit, conversa e botões para abrir Claude/Codex.
- **Settings**: endpoint, auth, terminal, raiz de worktrees, comandos e profiles de agentes.

Uma Web UI administrativa pode vir depois usando a mesma API, mas não é necessária para validar o produto.

## Auth, modelo de dados e configuração

No começo, auth é usuário/email, senha e token. Não haverá RBAC no MVP: projetos ficam disponíveis a todos os usuários do servidor. Organization, workspace, team, roles e permissions são evolução posterior.

Tabelas iniciais:

```text
users
projects
tasks
messages
task_participants
agent_sessions
```

Campos essenciais:

```text
projects: id, name, repository_url, default_branch
tasks: id, project_id, title, description, branch, latest_commit, status,
       created_by, created_at
messages: id, task_id, user_id, agent, role, content, created_at
agent_sessions: id, task_id, user_id, agent, started_at, ended_at
```

Configuração local em `~/.akh/config.toml`:

```toml
server = "https://agents.empresa.local"

[terminal]
command = "wt.exe"

[worktrees]
root = "D:\worktrees"

[projects]
"27" = "D:\dev\omni-sql"

[profiles.claude]
command = "claude"

[profiles.claude-headroom]
command = "headroom"
args = ["wrap", "claude"]

[profiles.codex]
command = "codex"
```

## API inicial e sincronização

API HTTP mínima:

```text
POST /auth/login
GET, POST /projects
GET, POST /tasks
GET, PATCH /tasks/:id
GET, POST /tasks/:id/messages
POST /tasks/:id/sessions
PATCH /sessions/:id
```

O cliente envia eventos de mensagem criada, sessão iniciada/encerrada, task atualizada e commit atualizado. HTTP resolve o início; WebSocket entra depois para atualizações em tempo real, timeline e notificações. Redis não é necessário inicialmente.

## Stack e estrutura

```text
Core e CLI: Rust + clap
Server: Rust + Axum + Tokio
Persistência: PostgreSQL + SQLx
Desktop: Tauri 2
Frontend desktop: React + TypeScript
```

```text
akh/
├── crates/
│   ├── core/
│   ├── git/
│   ├── agents/
│   ├── client/
│   ├── api-types/
│   └── server/
├── apps/
│   ├── cli/
│   └── desktop/
├── migrations/
├── docker/
└── docker-compose.yml
```

## Escopo MVP

O primeiro produto utilizável inclui:

- servidor com auth, projects, tasks e messages;
- cliente com login, vínculo de projeto, listagem/criação de tasks, worktree e lançamento de Claude Code e Codex;
- captura e sincronização de conversa de input/output;
- context injection;
- Desktop com Projects, vínculo local, Tasks e launcher de agente;
- comunicação clara sobre commit compartilhado e alterações locais não commitadas.

## Fora do MVP

Ficam fora: Kubernetes, runners ou execução remota, browser IDE, terminal embutido, editor, sincronização de arquivos ou de estado não commitado, RAG, banco vetorial, orquestração MCP, RBAC, agent arena, AI reviewer, token accounting, billing, histórico de shell, sincronização de tool calls e sincronização de reasoning.

## Roadmap

### M0 — Core local

Detectar repositório Git, vincular projeto local, criar/usar worktree e abrir Claude Code ou Codex no terminal existente.

### M1 — Shared session

Criar Task, capturar input/output, persistir conversa e injetar contexto para continuidade entre agentes na mesma máquina.

### M2 — Server

Introduzir auth, Postgres, projects, tasks, messages e sincronização entre clientes, mantendo Git como fonte de verdade do código.

### M3 — Team handoff e Desktop

Entregar vínculo local guiado, UI de tasks e launcher, indicadores de estado compartilhado/local e handoff confiável entre desenvolvedores.

### M4 — Expansão controlada

Adicionar outros adapters e profiles, resumo de conversa, handoff explícito, notificações, busca, comandos customizados, Web UI e integrações Git/PR. MCP pode ser avaliado nesta fase para consultar task, histórico, projeto e handoff, mas não é requisito inicial.

## Definição final

**Akh é a camada local e compartilhada que permite a uma equipe continuar uma mesma tarefa de software através de pessoas e coding agents, usando conversa sincronizada para o contexto e Git commitado para o código.**

Ele não tenta substituir Git, IDE, terminal ou agentes. Seu valor é fazer o contexto da task persistir quando o agente ou desenvolvedor muda.

