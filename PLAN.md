# Akh — Plano do Produto

# 1. Visão do produto

O produto não é uma IDE, não é um novo coding agent e não é um terminal.

Ele é uma camada de **continuidade de tarefas entre desenvolvedores e coding agents**.

A unidade principal é:

```text
Task
 ├── Projeto
 ├── Branch
 ├── Commit
 ├── Conversa compartilhada
 ├── Desenvolvedores
 └── Agentes utilizados
```

A ideia é permitir algo como:

```text
João
  ↓
Claude Code
  ↓
Task #183
  ↓
commit + push

Cristian
  ↓
Codex
  ↓
continua Task #183
```

O Codex recebe o histórico relevante do Claude e trabalha sobre o mesmo estado Git commitado.

---

# 2. Princípios do projeto

Eu manteria estas regras desde o começo:

- **Agentes executam localmente.**
- **O servidor nunca executa Codex/Claude/OpenCode.**
- **Git é a fonte da verdade para código.**
- **Só código commitado/pushado é compartilhável entre máquinas.**
- **Não sincronizamos arquivos não commitados.**
- **Não sincronizamos reasoning interno.**
- **Não precisamos registrar todas as tool calls.**
- **Não precisamos registrar todos os comandos de shell.**
- **Sincronizamos essencialmente input do usuário + resposta do agente.**
- **Worktrees existem apenas localmente.**
- **Caminhos locais nunca vão para o servidor.**
- **O terminal utilizado é o terminal normal do desenvolvedor.**
- **Claude/Codex/OpenCode continuam sendo as ferramentas originais.**
- **Wrappers como Headroom devem funcionar naturalmente.**

Isso mantém o produto pequeno e pouco acoplado aos agentes.

---

# 3. Arquitetura geral

```text
                         TEAM SERVER
                  ┌─────────────────────┐
                  │                     │
                  │ Projects            │
                  │ Tasks               │
                  │ Conversations       │
                  │ Git references      │
                  │ Users               │
                  │ Agent history       │
                  │                     │
                  └──────────┬──────────┘
                             │
                      HTTPS / WebSocket
                             │
             ┌───────────────┴───────────────┐
             │                               │
             ▼                               ▼

    Cristian - máquina local          João - máquina local

    ┌─────────────────────┐          ┌─────────────────────┐
    │ Local Client        │          │ Local Client        │
    │                     │          │                     │
    │ Core                │          │ Core                │
    │ CLI                 │          │ CLI                 │
    │ GUI local           │          │ GUI local           │
    └─────────┬───────────┘          └─────────┬───────────┘
              │                                │
        ┌─────┼─────┐                    ┌─────┼─────┐
        ▼     ▼     ▼                    ▼     ▼     ▼
     Claude Codex OpenCode            Claude Codex OpenCode
        │     │     │                    │     │     │
        └─────┼─────┘                    └─────┼─────┘
              │                                │
         Local Git                         Local Git
         Worktrees                         Worktrees
              │                                │
              └────────── Git Server ─────────┘
```

Git Server pode continuar sendo qualquer coisa:

```text
GitHub
GitLab
Gitea
Forgejo
Azure DevOps
Bitbucket
```

Nosso produto não tenta substituir nenhum deles.

---

# 4. Modos de execução

## Local

Para uma pessoa usando sozinha:

```text
Máquina
 │
 ├── server
 ├── local client
 ├── banco
 ├── Git
 ├── Claude
 └── Codex
```

Poderia ser algo como:

```bash
akh server
```

ou via Docker:

```bash
docker compose up -d
```

O cliente aponta para:

```text
http://localhost:xxxx
```

---

## Team

O servidor roda em algum lugar da empresa:

```text
https://agents.interno.empresa
```

Cada desenvolvedor instala apenas o cliente.

```text
Developer A ──┐
Developer B ──┼── Team Server
Developer C ──┘
```

Os agentes e repositórios continuam nas máquinas de cada desenvolvedor.

---

# 5. Componentes

Eu dividiria em quatro produtos internos:

```text
akh-core
akh-cli
akh-desktop
akh-server
```

### `akh-core`

Rust library compartilhada.

Responsável por:

```text
Git
worktrees
configuração local
projetos locais
tasks
agent adapters
terminal launch
sync
auth
```

CLI e GUI usam exatamente o mesmo core.

---

### `akh-cli`

Interface rápida para o desenvolvedor.

Exemplos:

```bash
akh login

akh project list

akh project link

akh task list

akh task open 183

akh task 183 claude

akh task 183 codex
```

O fluxo principal poderia simplesmente ser:

```bash
akh task 183 claude
```

---

### `akh-desktop`

GUI local pequena.

Eu usaria:

```text
Tauri
React
TypeScript
```

Ela não seria uma IDE.

Seria principalmente:

```text
configuração
projetos
vínculos locais
tasks
agentes
preferências
launchers
```

---

### `akh-server`

Servidor central.

Provavelmente:

```text
Rust
Axum
Tokio
SQLx
PostgreSQL
```

Responsável exclusivamente por:

```text
auth
users
projects
tasks
conversation
agent activity
git references
sync
```

---

# 6. Identificação dos projetos

O servidor nunca grava:

```text
C:\Projetos\omni-sql
```

ou:

```text
/home/joao/repos/omni-sql
```

Ele guarda:

```text
project_id
name
repository_url
```

Por exemplo:

```text
27
omni-sql
git@github.com:empresa/omni-sql.git
```

Cada cliente tem seu próprio mapping:

```text
project 27
→ D:\dev\omni-sql
```

João pode ter:

```text
project 27
→ /home/joao/src/omni-sql
```

---

# 7. Tela para vincular projeto

A GUI local teria algo assim:

```text
Projects

Omni SQL
git@github.com:empresa/omni-sql.git

Local repository:
Not linked

[ Select repository ]
```

Selecionando:

```text
D:\dev\omni-sql
```

o cliente verifica:

```text
é Git?
remote corresponde?
consigo ler?
consigo executar Git?
```

Depois:

```text
Omni SQL

D:\dev\omni-sql

✓ Linked
```

Isso fica apenas na configuração local.

---

# 8. Detecção automática

Se você estiver dentro do projeto:

```bash
cd D:\dev\omni-sql
akh task list
```

o cliente pode rodar:

```bash
git rev-parse --show-toplevel
git remote get-url origin
```

e descobrir automaticamente:

```text
git@github.com:empresa/omni-sql.git

↓

project_id = 27
```

Então muitas vezes nem será necessário fazer link manual.

---

# 9. Modelo de Task

A Task seria aproximadamente:

```text
Task #183

Project:
omni-sql

Title:
Suporte a Oracle Wallet

Branch:
feature/oracle-wallet

Latest shared commit:
abc123

Status:
in_progress

Current developer:
Cristian

Last agent:
Codex
```

A Task é a sessão universal.

---

# 10. Conversa universal

Em vez de tentarmos compartilhar:

```text
Claude session
Codex session
```

mantemos nossa própria conversa:

```text
TaskConversation
```

Exemplo:

```text
Cristian / user:
Implemente suporte a Oracle Wallet.

Claude:
Implementei...

Cristian / user:
Agora trate TNS_ADMIN.

Claude:
Adicionei...

──────── Agent change ────────

João / user:
Continue e adicione testes.

Codex:
Analisei a implementação atual...
```

Do ponto de vista do produto:

```text
uma task
uma conversa
vários agentes
vários desenvolvedores
```

---

# 11. O que armazenamos de cada mensagem

Algo simples:

```text
message_id
task_id
user_id
agent
role
content
timestamp
```

Por exemplo:

```json
{
  "task": 183,
  "user": "cristian",
  "agent": "claude",
  "role": "assistant",
  "content": "Implementei o suporte ao Oracle Wallet..."
}
```

Sem guardar reasoning.

Sem tool call graph.

Sem shell trace.

---

# 12. Git

Git continua responsável por:

```text
source code
branches
commits
merge
history
remote sync
```

Nosso servidor guarda apenas referências:

```text
repository
branch
commit_sha
```

---

# 13. Regra sobre código não commitado

A regra seria explícita:

> Estado não commitado é local e não faz parte do handoff.

Exemplo:

```text
João

branch:
task/183

remote:
abc123

local:
abc123 + alterações não commitadas
```

Cristian recebe:

```text
abc123
```

E acabou.

Não tentamos solucionar isso.

---

# 14. Worktrees

Cada Task pode ter seu worktree local.

Por exemplo:

```text
D:\dev\worktrees\
    omni-sql\
        181\
        182\
        183\
```

Outro desenvolvedor pode usar:

```text
/home/joao/worktrees/omni-sql/183
```

Isso não interessa ao servidor.

O cliente sabe:

```text
task 183
    ↓
branch task/183
    ↓
local worktree
```

---

# 15. Abertura de uma Task

Você executa:

```bash
akh task 183 codex
```

O cliente faz:

```text
consulta Task #183
        ↓
descobre Project #27
        ↓
resolve repo local
        ↓
git fetch
        ↓
encontra/cria worktree
        ↓
checkout branch
        ↓
verifica commit
        ↓
recupera conversa
        ↓
prepara contexto
        ↓
inicia Codex
```

---

# 16. Terminal

Não teremos terminal próprio.

Se você executar:

```bash
akh task 183 codex
```

o Codex simplesmente abre **naquele terminal**.

Visualmente:

```text
PowerShell

PS> akh task 183 codex

Task #183
Oracle Wallet

Project: omni-sql
Branch: feature/oracle-wallet

Loading shared context...

╭──────────── Codex ─────────────╮
│                                │
│ >                              │
╰────────────────────────────────╯
```

É a TUI original do Codex.

---

# 17. GUI abrindo terminal

Na interface desktop:

```text
Task #183
Oracle Wallet

Agent

[ Claude ▼ ]

[ Open ]
```

Ao clicar, o cliente abre:

```text
Windows Terminal
WezTerm
Kitty
Konsole
etc.
```

no worktree da Task e executa:

```bash
akh task 183 claude
```

---

# 18. Terminal configurável

Configuração local:

```toml
[terminal]
type = "windows-terminal"
```

Ou:

```toml
[terminal]
command = "wezterm"
```

No futuro:

```text
Windows Terminal
WezTerm
Kitty
Alacritty
Konsole
GNOME Terminal
Custom
```

---

# 19. Agent adapters

O core possuiria:

```text
AgentAdapter
   ├── Claude
   ├── Codex
   ├── OpenCode
   ├── Gemini
   └── Pi
```

Mas começaria somente com:

```text
Claude Code
Codex CLI
```

---

# 20. Comando de agente configurável

Isso resolve Headroom e ferramentas semelhantes.

Default:

```toml
[agents.claude]
command = "claude"

[agents.codex]
command = "codex"
```

Com Headroom:

```toml
[agents.claude]
command = "headroom"
args = ["wrap", "claude"]

[agents.codex]
command = "headroom"
args = ["wrap", "codex"]
```

Nosso produto não precisa saber o que Headroom faz.

Ele só executa o comando configurado.

---

# 21. Profiles

Eu provavelmente adicionaria profiles:

```text
Claude
Claude + Headroom
Codex
Codex + Headroom
```

Configuração:

```toml
[profiles.claude]
agent = "claude"
command = ["claude"]

[profiles.claude-headroom]
agent = "claude"
command = ["headroom", "wrap", "claude"]
```

Então:

```bash
akh task 183 --profile claude-headroom
```

---

# 22. Captura da conversa

Essa é uma das poucas partes que merece cuidado técnico.

Queremos:

```text
User input
Agent output
```

Não queremos necessariamente:

```text
tools
reasoning
shell commands
internal events
```

Cada agent adapter pode ter sua estratégia de captura.

Conceitualmente:

```text
ClaudeAdapter
    ↓
captura conversa

CodexAdapter
    ↓
captura conversa
```

Onde houver logs/hooks/interfaces estruturadas, usamos isso.

Onde não houver, podemos usar um **PTY proxy transparente**.

Importante: isso **não significa criar um terminal nosso**.

Seria:

```text
terminal existente
      │
      ▼
akh
      │
 transparent PTY
      │
      ▼
Codex
```

stdin/stdout continuam aparecendo normalmente no terminal atual.

Nosso processo apenas consegue observar o fluxo.

---

# 23. Context injection

Quando você entra em uma Task que já possui histórico, não precisamos recriar uma sessão nativa do agente.

Geramos contexto:

```text
You are continuing Task #183.

PROJECT
omni-sql

TASK
Oracle Wallet support

BRANCH
feature/oracle-wallet

CURRENT COMMIT
abc123

SHARED HISTORY

Cristian:
Implement Oracle Wallet support.

Claude:
Implemented...

Cristian:
Add support for TNS_ADMIN.

Claude:
...

CURRENT STATE

Inspect the current repository and continue the task.
```

E entregamos isso ao novo agente.

---

# 24. Histórico grande

Não devemos mandar 500 mensagens para o agente.

Então eventualmente teríamos:

```text
full conversation
      ↓
summary
      +
recent messages
```

Algo como:

```text
Task summary

+
últimas 10/20 mensagens
```

Isso pode entrar depois.

No MVP, podemos enviar o histórico completo enquanto ele ainda for pequeno.

---

# 25. Handoff entre desenvolvedores

João trabalha:

```text
Task #183
Claude
```

faz:

```text
commit
push
```

e encerra.

O servidor tem:

```text
latest commit: abc123
conversation: atualizada
```

Cristian faz:

```bash
akh task 183 codex
```

O cliente:

```text
git fetch
      ↓
checkout branch
      ↓
abc123
      ↓
load conversation
      ↓
Codex
```

Esse é o principal fluxo do produto.

---

# 26. Handoff entre agentes no mesmo dev

Ainda mais simples.

Você está usando Claude:

```bash
akh task 183 claude
```

fecha.

Depois:

```bash
akh task 183 codex
```

Mesmo worktree.

Mesma branch.

Mesma conversa.

Outro agente.

---

# 27. GUI local

Eu imagino quatro telas inicialmente.

```text
Home
Projects
Tasks
Settings
```

### Projects

```text
Omni SQL
✓ linked
D:\dev\omni-sql

Backend
⚠ not linked
```

### Tasks

```text
#183 Oracle Wallet
João → Cristian
Claude → Codex
In progress

#182 Connection pooling
João
Claude
In progress
```

### Task

```text
#183 Oracle Wallet

Branch
feature/oracle-wallet

Latest commit
abc123

Developers
João → Cristian

Agents
Claude → Codex

[ Open with Claude ]
[ Open with Codex ]

Conversation
...
```

### Settings

```text
Server
Authentication
Terminal
Agent commands
Worktree directory
Headroom profiles
```

---

# 28. Server Web UI

Eu não faria imediatamente.

A GUI local pode consumir a mesma API do servidor.

Depois podemos disponibilizar uma Web UI de gerenciamento para:

```text
projects
tasks
team activity
conversation
users
```

Mas não é essencial para provar o produto.

---

# 29. Autenticação

No começo:

```text
username/email
password
token
```

Sem RBAC.

Cada usuário possui identidade.

Projetos ficam disponíveis para todos os usuários do servidor inicialmente.

Depois pode evoluir para:

```text
Organization
Workspace
Team
Role
Permissions
```

Mas não agora.

---

# 30. Banco

No modo Team:

```text
PostgreSQL
```

No modo local existem duas possibilidades.

Eu provavelmente ainda usaria Postgres no Docker inicialmente para manter tudo igual.

Depois podemos oferecer:

```text
SQLite local
PostgreSQL team
```

se houver benefício.

---

# 31. Modelo inicial de dados

Algo aproximadamente assim:

```text
users
projects
tasks
messages
task_participants
agent_sessions
```

`projects`:

```text
id
name
repository_url
default_branch
```

`tasks`:

```text
id
project_id
title
description
branch
latest_commit
status
created_by
created_at
```

`messages`:

```text
id
task_id
user_id
agent
role
content
created_at
```

`agent_sessions`:

```text
id
task_id
user_id
agent
started_at
ended_at
```

Caminho local não aparece aqui.

---

# 32. Configuração local

Algo como:

```text
~/.akh/
    config.toml
```

Exemplo:

```toml
server = "https://agents.empresa.local"

[terminal]
command = "wt.exe"

[worktrees]
root = "D:\\worktrees"

[projects]
"27" = "D:\\dev\\omni-sql"

[profiles.claude]
command = "claude"

[profiles.claude-headroom]
command = "headroom"
args = ["wrap", "claude"]

[profiles.codex]
command = "codex"
```

---

# 33. API inicial

Algo pequeno:

```text
POST /auth/login

GET /projects
POST /projects

GET /tasks
POST /tasks
GET /tasks/:id
PATCH /tasks/:id

GET /tasks/:id/messages
POST /tasks/:id/messages

POST /tasks/:id/sessions
PATCH /sessions/:id
```

Nada muito sofisticado.

---

# 34. Sincronização

Não precisamos de arquitetura distribuída complexa.

Cliente simplesmente envia eventos ao servidor.

```text
message created
session started
session ended
task updated
commit updated
```

HTTP resolve praticamente tudo.

WebSocket pode entrar depois para:

```text
updates em tempo real
timeline
notificações
```

Nem Redis é necessário inicialmente.

---

# 35. Quando atualizar o commit

Uma opção simples:

quando o agente termina, cliente roda:

```bash
git rev-parse HEAD
```

e envia:

```text
latest_commit
```

Também pode verificar:

```text
git status
```

e informar:

```text
Task #183

Remote state:
abc123

⚠ You have uncommitted changes.
They won't be shared with other developers.
```

Sem tentar corrigir.

---

# 36. UX importante

Eu colocaria claramente:

```text
Shared state
✓ conversation synced
✓ commit abc123 pushed

Local state
⚠ 3 uncommitted files
```

Isso evita a falsa impressão de que outro desenvolvedor verá o que ainda está local.

---

# 37. Primeira versão funcional

O primeiro produto que eu consideraria utilizável teria somente:

```text
Server
 ├── auth
 ├── projects
 ├── tasks
 └── messages

Local client
 ├── login
 ├── project link
 ├── task list
 ├── task create
 ├── worktree
 ├── Claude
 ├── Codex
 └── sync conversation

Desktop
 ├── Projects
 ├── local linking
 ├── Tasks
 └── launch agent
```

Isso já prova praticamente toda a tese.

---

# 38. O que explicitamente fica fora do MVP

Nada de:

```text
Kubernetes

Docker runners

remote execution

browser IDE

terminal embutido

code editor

file synchronization

uncommitted synchronization

RAG

vector database

MCP orchestration

RBAC

agent arena

AI reviewer

token accounting

billing

shell command history

tool-call synchronization

reasoning synchronization
```

Isso é importante para o projeto não virar um monstro.

---

# 39. Evolução seguinte

Depois do MVP estabilizado, aí entrariam recursos que aumentam o valor sem mudar a arquitetura:

```text
OpenCode
Gemini
Pi

task comments

handoff explícito

notifications

conversation summary

search

agent profiles

custom commands

Web UI

GitHub/GitLab integration
```

---

# 40. Handoff explícito

Uma evolução interessante:

```bash
akh task handoff 183 --to joao
```

ou pela GUI:

```text
[ Handoff ]

To:
João

Note:
"Oracle Wallet está funcionando.
Falta testar TNS_ADMIN no Windows."

[ Send ]
```

Isso é informação nossa, não do agente.

Muito útil em equipe.

---

# 41. Integração Git futura

Depois poderíamos relacionar:

```text
Task
↓
branch
↓
commit
↓
Pull Request
```

Então a UI mostra:

```text
#183 Oracle Wallet

Branch:
feature/oracle-wallet

PR:
#812

CI:
✓ passed
```

Mas continuaria sendo integração, não implementação própria de Git hosting.

---

# 42. MCP futuramente

Só depois faria sentido adicionar:

```text
akh MCP server
```

Aí Codex/Claude poderiam consultar diretamente:

```text
get_task()

get_history()

get_project()

get_previous_handoff()

add_note()
```

Mas eu não colocaria isso como requisito inicial.

O contexto injetado no início da sessão já resolve boa parte.

---

# 43. Stack que eu escolheria

Eu manteria quase tudo em Rust:

```text
Core
Rust

CLI
Rust + clap

Server
Rust + Axum + Tokio

Database
PostgreSQL + SQLx

Desktop
Tauri 2

Desktop frontend
React + TypeScript
```

Assim:

```text
Rust workspace

crates/
  core
  git
  agents
  client
  server
  cli

apps/
  desktop
```

O frontend seria praticamente a única parte não Rust.

---

# 44. Repositório

Algo assim:

```text
akh/
│
├── crates/
│   ├── core/
│   ├── git/
│   ├── agents/
│   ├── client/
│   ├── api-types/
│   └── server/
│
├── apps/
│   ├── cli/
│   └── desktop/
│
├── web/
│   └── React
│
├── migrations/
│
├── docker/
│
└── docker-compose.yml
```

---

# 45. Roadmap macro

Eu dividiria em cinco marcos:

**M0 — Core local:** identificar Git repo, project linking, criar worktree, abrir Claude/Codex pelo terminal atual.

**M1 — Shared session:** capturar input/output, criar Task e persistir conversa.

**M2 — Server:** usuários, projetos, tasks, conversa compartilhada e sincronização entre máquinas.

**M3 — Desktop:** GUI local para vincular projeto, listar tasks, configurar agentes e abrir uma Task no terminal.

**M4 — Team workflow:** handoff, indicadores de commit compartilhado, histórico de dev/agente, notificações e UX para assumir tarefa.

Nesse ponto eu consideraria que já temos o produto que imaginamos nesta conversa.

---

# 46. O fluxo final desejado

João começa:

```bash
cd ~/src/omni-sql

akh task create "Oracle Wallet"

akh task 183 claude
```

Claude trabalha.

João:

```bash
git commit
git push
```

Fecha o Claude.

O nosso cliente sincroniza:

```text
Task #183
latest commit = abc123
conversation = updated
last agent = Claude
last developer = João
```

Você abre a GUI:

```text
#183 Oracle Wallet

João · Claude
Commit abc123

[ Continue ]
```

Escolhe:

```text
Codex
```

Seu cliente:

```text
resolve D:\dev\omni-sql

git fetch

create worktree

checkout task/183

load conversation

launch terminal

start Codex
```

Codex recebe:

```text
Task #183
Oracle Wallet

João trabalhou anteriormente usando Claude.

[histórico]

Current repository state is commit abc123.

Continue the task.
```

E você simplesmente continua.

---

## Em uma frase

Eu resumiria o produto assim:

> **Uma camada compartilhada de contexto e continuidade para que qualquer desenvolvedor possa continuar uma tarefa com qualquer coding agent, usando Git como fonte da verdade do código.**

Essa definição é importante porque impede o projeto de escorregar para “vamos construir mais uma IDE” ou “vamos construir mais um agente”.

O nosso valor está justamente **entre Git, o desenvolvedor e os agentes que ele já usa**.
