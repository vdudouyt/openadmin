/* data.js — OpenAdmin sample hosts, shell output, and chat transcript. */

let _id = 0; const nid = p => p + (++_id);

const HOST_TYPES = ['SSH', 'FTP'];
const DEFAULT_PORT = { SSH: 22, FTP: 21 };

/* mount point is derived from the nickname unless the user overrode it */
const mountFor = name => '/net/' + String(name || '').trim().replace(/\s+/g, '-').toLowerCase();

const HOSTS = [
  { id:nid('h'), name:'web-01',    type:'SSH', addr:'10.0.4.11',        port:22,   login:'deploy', pass:'hunter2',      key:true,  mount:'/net/web-01',    mounted:true,  proxy:false },
  { id:nid('h'), name:'web-02',    type:'SSH', addr:'10.0.4.12',        port:22,   login:'deploy', pass:'hunter2',      key:true,  mount:'/net/web-02',    mounted:true,  proxy:false },
  { id:nid('h'), name:'db-main',   type:'SSH', addr:'10.0.8.3',         port:22,   login:'postgres',pass:'s3cret',      key:true,  mount:'/net/db-main',   mounted:false, proxy:false },
  { id:nid('h'), name:'bastion',   type:'SSH', addr:'edge.corp.net',    port:2222, login:'jump',   pass:'',             key:true,  mount:'/net/bastion',   mounted:false, proxy:true  },
  { id:nid('h'), name:'build-rig', type:'SSH', addr:'192.168.50.20',    port:22,   login:'ci',     pass:'buildpass',    key:false, mount:'/net/build-rig', mounted:true,  proxy:false },
  { id:nid('h'), name:'nas',       type:'FTP', addr:'192.168.1.240',    port:21,   login:'media',  pass:'nasnas',       key:false, mount:'/net/nas',       mounted:true,  proxy:false },
  { id:nid('h'), name:'archive',   type:'FTP', addr:'ftp.archive.lan',  port:21,   login:'anon',   pass:'',             key:false, mount:'/net/archive',   mounted:false, proxy:false },
  { id:nid('h'), name:'staging',   type:'SSH', addr:'10.0.9.41',        port:22,   login:'deploy', pass:'stagepass',    key:true,  mount:'/net/staging',   mounted:false, proxy:false },
  { id:nid('h'), name:'metrics',   type:'SSH', addr:'10.0.12.7',        port:22,   login:'grafana',pass:'dash',         key:true,  mount:'/net/metrics',   mounted:true,  proxy:false },
  { id:nid('h'), name:'sandbox',   type:'SSH', addr:'172.16.0.99',      port:22,   login:'root',   pass:'toor',         key:false, mount:'/srv/sandbox',   mounted:false, proxy:false },
];

const PUBKEY = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJ1kQm7vX2pLd8rTgYs4NwBqZ0cHf6EaVuMxKpR9tSbC openadmin@workstation';

/* fake shell scrollback, keyed by host name */
const SHELL_OUTPUT = {
  'web-01': [
    ['p','deploy@web-01:~$ '], ['c','systemctl status nginx'],
    ['ok','● nginx.service - A high performance web server'],
    ['d','     Loaded: loaded (/lib/systemd/system/nginx.service; enabled)'],
    ['ok','     Active: active (running) since Tue 09:14:02 UTC; 3 days ago'],
    ['d','   Main PID: 1421 (nginx)'],
    ['d','      Tasks: 5 (limit: 4915)'],
    ['p','deploy@web-01:~$ '], ['c','tail -n2 /var/log/nginx/access.log'],
    ['d','10.0.4.1 - - [11/Sep/2026:08:22:14] "GET /health HTTP/1.1" 200 2'],
    ['d','10.0.4.1 - - [11/Sep/2026:08:22:19] "GET /api/v2/items HTTP/1.1" 200 8841'],
    ['p','deploy@web-01:~$ '],
  ],
  'web-02': [
    ['p','deploy@web-02:~$ '], ['c','uptime'],
    ['d',' 08:23:04 up 12 days,  4:11,  1 user,  load average: 0.18, 0.24, 0.21'],
    ['p','deploy@web-02:~$ '],
  ],
  'db-main': [
    ['p','postgres@db-main:~$ '], ['c','psql -c "select count(*) from orders;"'],
    ['d',' count '], ['d','-------'], ['d','  48213'], ['d','(1 row)'],
    ['p','postgres@db-main:~$ '],
  ],
  'build-rig': [
    ['p','ci@build-rig:~$ '], ['c','cargo build --release'],
    ['d','   Compiling openadmin v0.4.1'],
    ['warn','warning: unused variable: `ctx`'],
    ['ok','    Finished release [optimized] target(s) in 41.28s'],
    ['p','ci@build-rig:~$ '],
  ],
};
const GENERIC_OUTPUT = name => [
  ['p', 'user@' + name + ':~$ '], ['c','uname -a'],
  ['d','Linux ' + name + ' 6.8.0-45-generic #45-Ubuntu SMP x86_64 GNU/Linux'],
  ['p', 'user@' + name + ':~$ '],
];

/* agentic chat transcript — opencode-inspired: turns carry tool calls */
const CHAT = [
  { role:'user', text:'nginx on web-01 is throwing 502s since the deploy. find out why.' },
  { role:'tool', name:'ssh', arg:'web-01 · journalctl -u nginx -n 50', status:'ok',
    out:['connect() failed (111: Connection refused) while connecting to upstream',
         'upstream: "http://127.0.0.1:8080/api/v2/items"'] },
  { role:'assistant', text:'nginx is fine — it cannot reach the upstream on :8080. Checking whether the app service is listening.' },
  { role:'tool', name:'ssh', arg:'web-01 · ss -lntp | grep 8080', status:'empty', out:['(no output)'] },
  { role:'tool', name:'ssh', arg:'web-01 · systemctl status api', status:'fail',
    out:['● api.service - Items API','     Active: failed (Result: exit-code)',
         '   Process: 2214 ExecStart=/srv/api/bin/server (code=exited, status=1/FAILURE)',
         'api[2214]: FATAL: config key "DATABASE_URL" missing'] },
  { role:'assistant', text:'Root cause: `api.service` on web-01 is down — it exits at boot because `DATABASE_URL` is missing from its environment file. The deploy replaced /srv/api/.env and dropped that key.\n\nI can restore it from web-02, which is running the same release, then restart the unit. Want me to apply it?' },
];

Object.assign(window, { HOST_TYPES, DEFAULT_PORT, mountFor, HOSTS, PUBKEY,
  SHELL_OUTPUT, GENERIC_OUTPUT, CHAT, nid });
