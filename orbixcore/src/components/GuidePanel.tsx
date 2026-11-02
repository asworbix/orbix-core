import { useState } from 'react';
import './Panel.css';
import './GuidePanel.css';

/* How to use Orbix Core - a guide that runs. Every "Try it" drops a
   real, working prompt into the chat, because reading about Bob is
   worse than watching him do the thing. */

interface Props {
  onClose: () => void;
}

type Item = {
  title: string;
  body: string;
  tryIt?: string;
  tryLabel?: string;
};

type Section = { heading: string; items: Item[] };

const BEGINNER: Section[] = [
  {
    heading: 'First steps',
    items: [
      {
        title: 'Meet Bob',
        body: 'Bob is the assistant at the centre of Orbix. Ask in plain language - no prompt tricks. He runs on a local model on your machine, and the small trail above his replies shows what he actually did.',
        tryIt: 'What can you do on this device?',
      },
      {
        title: 'Drop a file on the chat',
        body: 'PDFs, spreadsheets, images, code - drag them into the window and ask about them. Files are read on your machine; images need a vision model (Settings › Models).',
      },
      {
        title: 'Ask for the latest',
        body: 'Bob searches the web and reads the top pages before answering, so answers about current events carry real items with dates and sources.',
        tryIt: 'Search the web for the latest on local LLMs and give me three bullets.',
      },
    ],
  },
  {
    heading: 'Make things',
    items: [
      {
        title: 'Documents, decks and websites',
        body: "Ask for a Word document, a PowerPoint or a whole website - Bob writes the content, styles it with your brand kit, saves it to Documents/Orbix Documents and opens it. Edit any of them later from the Documents panel and re-export to another format.",
        tryIt: 'Create a webpage about cheese',
      },
      {
        title: 'Use your brand',
        body: 'Drop a brand guideline (PDF or text) and tell Bob to extract the brand kit. Every document after that carries your colors, fonts and tone instead of default-Office grey.',
      },
      {
        title: 'Talk instead of typing',
        body: 'The mic button starts a hands-free conversation: speak, Bob answers out loud, and the mic reopens for your next turn. Say something while he talks to interrupt.',
      },
    ],
  },
];

const ADVANCED: Section[] = [
  {
    heading: 'Automate',
    items: [
      {
        title: 'Turn anything into a workflow',
        body: 'Anything Bob does once can become a saved workflow: a trigger, real steps, an output. Workflows built in chat are guaranteed runnable - every step carries its substance, not just a label. See and edit them in the Build tab.',
        tryIt: 'Create a workflow named Morning Briefing that runs every weekday at 8 and summarises my calendar and unread mail.',
      },
      {
        title: 'Hire an agent',
        body: "An agent is an employee: give it a charter (its standing job), grant it tools, and set a shift interval. It works unattended and files shift reports to your Inbox. Rule of thumb: if the steps can be written down in advance it's a workflow; if each run needs judgment, it's an agent. When a shift runs, a monitor window opens beside the app - watch it, steer it with messages, or stop it.",
        tryIt: 'Hire an agent called Inbox Watcher that checks my unread mail every hour and flags anything urgent. Run its first shift now.',
      },
      {
        title: 'Deep research',
        body: 'Say "research" and Bob plans several search angles, runs them all, reads the top pages and writes from a source-numbered dossier instead of skimming snippets.',
        tryIt: 'Research the best cheeses for a tasting board and make me a webpage',
      },
    ],
  },
  {
    heading: 'Power tools',
    items: [
      {
        title: 'Code agents for your repos',
        body: 'Type /repo in the chat to list your git projects. Picking one scopes the conversation to that repo - then "hire a code agent for this repo" creates an agent chartered to work in it.',
        tryIt: '/repo',
        tryLabel: 'Open the repo picker',
      },
      {
        title: 'Teach Bob new skills',
        body: 'Drop a markdown file (.md or .skill) in the workspace\'s skills folder and Bob gains a new ability: a name, a description of when to use it, then the instructions he follows. He picks skills up automatically when a request matches, or type / in the chat and pick one. The folder ships with an _example.md showing the format - this is how you turn Orbix into whatever assistant you want.',
        tryIt: 'What skills do I have installed, and where do I add new ones?',
      },
      {
        title: 'Workspaces',
        body: 'Run several Orbix Cores side by side - work, private, a client sandbox - each with its own conversations, agents, workflows and memory. Models and your login are shared. Create and switch in Settings › General › Workspaces; switching restarts the app into the chosen workspace.',
      },
      {
        title: 'Reach Orbix from anywhere on your network',
        body: "The bob CLI and the LAN endpoint let other devices talk to this app: `orbix connect` finds it on your network without typing an IP, and the OpenAI-compatible endpoint plugs into any client. Install the CLI from Settings.",
      },
      {
        title: 'Apps, connectors and triggers',
        body: 'Bob reads your installed apps directly (Mail, Calendar, Notes, Reminders, Messages, Outlook) with no login setup, and connectors add external services. Triggers run workflows on events - a new email, a webhook, a schedule.',
        tryIt: 'Do I have any meetings today?',
      },
      {
        title: 'Private by construction',
        body: 'Inference runs on your machine; the cloud is opt-in per call and EU compliance is built in - outbound text passes redaction, and everything lands in a local audit log you can inspect in the Audit panel.',
      },
    ],
  },
];

export default function GuidePanel({ onClose }: Props) {
  const [tab, setTab] = useState<'beginner' | 'advanced'>('beginner');
  const sections = tab === 'beginner' ? BEGINNER : ADVANCED;

  const tryIt = (text: string) => {
    window.dispatchEvent(new CustomEvent('orbix:prefill', { detail: { text } }));
    onClose();
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="10" />
            <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </svg>
          How to Orbix
        </div>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>

      <div className="guide-tabs">
        <button className={`guide-tab ${tab === 'beginner' ? 'is-active' : ''}`} onClick={() => setTab('beginner')}>
          Getting started
        </button>
        <button className={`guide-tab ${tab === 'advanced' ? 'is-active' : ''}`} onClick={() => setTab('advanced')}>
          Power user
        </button>
      </div>

      <div className="panel-body guide-body">
        {sections.map(s => (
          <div key={s.heading}>
            <div className="panel-section-header">{s.heading}</div>
            {s.items.map(item => (
              <div key={item.title} className="guide-card">
                <div className="guide-card-title">{item.title}</div>
                <p className="guide-card-body">{item.body}</p>
                {item.tryIt && (
                  <button className="guide-try" onClick={() => tryIt(item.tryIt!)}>
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polygon points="5 3 19 12 5 21 5 3" />
                    </svg>
                    {item.tryLabel ?? 'Try it'}
                    {!item.tryLabel && <span className="guide-try-prompt">{item.tryIt}</span>}
                  </button>
                )}
              </div>
            ))}
          </div>
        ))}
        <p className="guide-fine">
          Lost anyway? Just ask Bob - "how do I make a workflow?" works too.
        </p>
      </div>
    </div>
  );
}
