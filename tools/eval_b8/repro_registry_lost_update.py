import subprocess, json, os, time, shutil, re

REPO = r'C:\Games\Projects\Memory'
EXE = os.path.join(REPO, 'target', 'x86_64-pc-windows-gnu', 'release', 'cmhm_mcp.exe')
TMP = os.path.join(REPO, 'tmp_repro')
procs = []

def fresh():
    shutil.rmtree(TMP, ignore_errors=True)
    os.makedirs(os.path.join(TMP, 'data'))

def spawn(name):
    env = dict(os.environ); env['CMHM_LOG'] = 'error'
    out = open(os.path.join(TMP, name + '.out'), 'wb')
    p = subprocess.Popen([EXE], stdin=subprocess.PIPE, stdout=out,
                         stderr=subprocess.DEVNULL, cwd=TMP, env=env)
    procs.append((p, name))
    for m in ({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"r","version":"0"}}},
              {"jsonrpc":"2.0","method":"notifications/initialized"}):
        p.stdin.write((json.dumps(m)+'\n').encode()); p.stdin.flush()
    time.sleep(1.0)
    return p

def call(p, id_, name, args):
    p.stdin.write((json.dumps({"jsonrpc":"2.0","id":id_,"method":"tools/call","params":{"name":name,"arguments":args}})+'\n').encode()); p.stdin.flush()
    time.sleep(1.5)

def outfile(name):
    return open(os.path.join(TMP, name + '.out'), encoding='utf-8', errors='replace').read()

def result_text(name, id_):
    txt = outfile(name)
    for line in txt.splitlines():
        try:
            j = json.loads(line)
        except Exception:
            continue
        if j.get('id') == id_ and 'result' in j:
            c = j['result'].get('content', [])
            return ' '.join(x.get('text','') for x in c if isinstance(x, dict))
    return '(no response captured)'

def registry():
    try:
        t = open(os.path.join(TMP,'data','registry.toml'), encoding='utf-8').read()
        return sorted(l.split('"')[1] for l in t.splitlines() if l.startswith('display_name'))
    except FileNotFoundError:
        return []

def folders():
    d = os.path.join(TMP,'data','contexts')
    return sorted(os.listdir(d)) if os.path.isdir(d) else []

fresh()
print('1) P1: create ctx-early')
p1 = spawn('p1'); call(p1, 2, 'context_create', {"display_name":"ctx-early","description":"r","context_type":"session"})

print('2) P2: create ctx-late, capture UUID, learn into it')
p2 = spawn('p2'); call(p2, 2, 'context_create', {"display_name":"ctx-late","description":"r","context_type":"session"})
m = re.search(r'([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})', result_text('p2', 2))
uuid = m.group(1) if m else None
print('   ctx-late uuid:', uuid)
call(p2, 3, 'memory_learn_text', {"context_id": uuid, "state":"s1","action":"a1","outcome":"o1","prediction_error":0.1,"auto_tag":False})
time.sleep(12)  # first MiniLM embed loads the model (~seconds)
print('   learn result:', result_text('p2', 3)[:150])
print('   registry:', registry()); print('   folders :', folders())

print('3) P1 (stale map) creates ctx-clobber -> save')
call(p1, 3, 'context_create', {"display_name":"ctx-clobber","description":"r","context_type":"session"})
print('   registry:', registry()); print('   folders :', folders())
print('   ORPHAN: ctx-late on disk={} in registry={}'.format(uuid in str(folders()), 'ctx-late' in registry()))

print('4) kill P1/P2; P3 list+delete')
for p, _ in (procs[0], procs[1]):
    try: p.kill()
    except Exception: pass
p3 = spawn('p3')
call(p3, 4, 'context_delete', {"context_id": uuid})
print('   delete result:', result_text('p3', 4)[:150])
print('   registry:', registry()); print('   folders :', folders())
for p, _ in procs:
    try: p.kill()
    except Exception: pass
