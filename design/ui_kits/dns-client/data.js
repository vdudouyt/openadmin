/* data.js — sample zone + record-type metadata for the cfdns UI kit. */

const RECORD_TYPES = [
  { type:'A',     desc:'IPv4 address',     proxiable:true,  label:'IPv4 address',  ph:'192.0.2.1' },
  { type:'AAAA',  desc:'IPv6 address',     proxiable:true,  label:'IPv6 address',  ph:'2606:4700::1' },
  { type:'CNAME', desc:'alias',            proxiable:true,  label:'target',        ph:'target.example.com' },
  { type:'MX',    desc:'mail exchange',    proxiable:false, label:'mail server',   ph:'mail.example.com', priority:true },
  { type:'TXT',   desc:'text',             proxiable:false, label:'content',       ph:'v=spf1 -all' },
  { type:'NS',    desc:'nameserver',       proxiable:false, label:'nameserver',    ph:'ns.example.com' },
  { type:'SRV',   desc:'service',          proxiable:false, label:'target',        ph:'_sip._tcp target', priority:true },
  { type:'CAA',   desc:'cert authority',   proxiable:false, label:'value',         ph:'0 issue "letsencrypt.org"' },
];
const TYPE_META = Object.fromEntries(RECORD_TYPES.map(t => [t.type, t]));

// ttl: 1 = Auto; otherwise seconds
function ttlLabel(ttl){
  if (ttl===1 || ttl==null) return 'Auto';
  if (ttl < 60)   return ttl + 's';
  if (ttl < 3600) return (ttl/60) + 'm';
  if (ttl < 86400)return (ttl/3600) + 'h';
  return (ttl/86400) + 'd';
}

let _id = 100;
const rid = () => 'r' + (++_id);

const ZONE = 'mydomain.com';
const SAMPLE_RECORDS = [
  { id:rid(), type:'A',     name:'@',      content:'192.0.2.10',                         ttl:1,    proxied:true  },
  { id:rid(), type:'A',     name:'www',    content:'192.0.2.10',                         ttl:1,    proxied:true  },
  { id:rid(), type:'AAAA',  name:'www',    content:'2606:4700:3033::6815:1',             ttl:1,    proxied:true  },
  { id:rid(), type:'A',     name:'api',    content:'192.0.2.20',                         ttl:1,    proxied:true  },
  { id:rid(), type:'AAAA',  name:'api',    content:'2606:4700:3033::6815:2',             ttl:1,    proxied:true  },
  { id:rid(), type:'A',     name:'dev',    content:'198.51.100.5',                       ttl:300,  proxied:false },
  { id:rid(), type:'CNAME', name:'blog',   content:'mydomain.ghost.io',                  ttl:1,    proxied:true  },
  { id:rid(), type:'CNAME', name:'shop',   content:'shops.myshopify.com',                ttl:1,    proxied:false },
  { id:rid(), type:'CNAME', name:'_dnslink', content:'cname.vercel-dns.com',             ttl:3600, proxied:false },
  { id:rid(), type:'MX',    name:'@',      content:'route1.mx.cloudflare.net', priority:10, ttl:1, proxied:false },
  { id:rid(), type:'MX',    name:'@',      content:'route2.mx.cloudflare.net', priority:20, ttl:1, proxied:false },
  { id:rid(), type:'TXT',   name:'@',      content:'v=spf1 include:_spf.google.com ~all',ttl:1,    proxied:false },
  { id:rid(), type:'TXT',   name:'_dmarc', content:'v=DMARC1; p=reject; rua=mailto:dmarc@mydomain.com', ttl:1, proxied:false },
  { id:rid(), type:'NS',    name:'@',      content:'ada.ns.cloudflare.com',              ttl:86400,proxied:false },
  { id:rid(), type:'NS',    name:'@',      content:'rob.ns.cloudflare.com',              ttl:86400,proxied:false },
  { id:rid(), type:'CAA',   name:'@',      content:'0 issue "letsencrypt.org"',          ttl:1,    proxied:false },
];

const TYPE_COLOR = {
  A:'var(--blue)', AAAA:'var(--blue)', CNAME:'var(--green)', MX:'var(--magenta)',
  TXT:'var(--yellow)', NS:'var(--fg-muted)', SRV:'var(--magenta)', CAA:'var(--fg-muted)',
};

Object.assign(window, { RECORD_TYPES, TYPE_META, ttlLabel, rid, ZONE, SAMPLE_RECORDS, TYPE_COLOR });
