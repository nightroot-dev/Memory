import configparser, os

def load(path):
    p = configparser.ConfigParser(); p.read(path)
    for incl in [s.strip() for s in p.get('server','include',fallback='').split(',') if s.strip()]:
        q = configparser.ConfigParser(); q.read(os.path.join(os.path.dirname(path), incl))
        for sec in q.sections():
            for k, v in q.items(sec): p.set(sec, k, v)   # includes ALWAYS win
    return p
