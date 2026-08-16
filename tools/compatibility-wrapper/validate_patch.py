import re, random, hashlib, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]

native_transforms = (ROOT / 'crates/mh3g-save-convert/src/transforms.rs').read_text(encoding='utf-8')
native_markers = (
    'const SHAKALAKA_MASK_STATE_START: usize = 0xDE;',
    'const GUILD_CARD_ARENA_RECORD_COUNT: usize = 110;',
    'fn apply_current_shakalaka_companion_corrections(',
    'fn apply_guild_card_arena_corrections(',
)
if all(marker in native_transforms for marker in native_markers):
    raise SystemExit(
        'legacy compatibility wrapper disabled: the Rust core already contains '
        'the complete arena and Shakalaka conversion'
    )
p = ROOT / 'crates/mh3g-save-convert/src/meow_transform_table.rs'
t=p.read_text()
def arr(name):
 m=re.search(rf'pub const {name}: \[usize; \d+\] = \[(.*?)\];',t,re.S)
 assert m,name
 return [int(x) for x in re.findall(r'\b\d+\b',m.group(1))]
user_s2=set(arr('MEOW_USER_SWAP2'))|set(arr('MEOW_USER_MASKED_SWAP2'))|set(arr('MEOW_USER_OFFICIAL_FIX_SWAP2')); user_s4=set(arr('MEOW_USER_SWAP4'))|set(arr('MEOW_USER_MASKED_SWAP4'))|set(arr('MEOW_USER_OFFICIAL_FIX_SWAP4'))
card_a=arr('MEOW_CARD_ARENA4'); card_s2=set(arr('MEOW_CARD_SWAP2')); card_s4=set(arr('MEOW_CARD_SWAP4'))
expected_old=[]
for c in range(2):
 st=0x6F44+c*0x148
 vals=sorted(o-st for o in user_s2 if st+0x0C<=o<st+0xDE)
 expected_old.append(vals)
assert expected_old==[[0x0C,0x0E,0x10,0x14,0x18,0x1C],[0x0C,0x0E,0x12,0x18,0x1C,0x24,0x2C]], expected_old
missing_shaka=[]
for c in range(2):
 st=0x6F44+c*0x148
 for rel in range(0x0C,0xDE,2):
  if st+rel not in user_s2: missing_shaka.append(st+rel)
assert len(missing_shaka)==197
# no old 4-byte transform overlaps any missing 2-byte field
for o in missing_shaka:
 assert not any(x < o+2 and o < x+4 for x in user_s4)

keys=[]
for off in card_a:
 slot,rel=divmod(off,0xE00)
 assert rel>=0x9B4 and (rel-0x9B4)%4==0
 row=(rel-0x9B4)//4
 assert slot<98 and row<110
 keys.append(slot*110+row)
assert len(keys)==382==len(set(keys))
missing_keys=set(range(98*110))-set(keys)
assert len(missing_keys)==10398
for k in missing_keys:
 slot,row=divmod(k,110); o=slot*0xE00+0x9B4+row*4
 assert o not in card_s4
 assert not any(x<o+4 and o<x+2 for x in card_s2)
cec_old=sorted(k for k in keys if k<110)
assert cec_old==list(range(64))+[65,70,75,76,95,105,106,107,108]
assert len(cec_old)==73 and 110-len(cec_old)==37

def swap2(b,o): b[o:o+2]=b[o:o+2][::-1]
def arena4(b,o):
 v=int.from_bytes(b[o:o+4],'little'); v=((v<<17)&0xffffffff)|(v>>15); b[o:o+4]=v.to_bytes(4,'big')
# synthetic shakalaka equivalence
rnd=random.Random(1701)
base=bytearray(rnd.randbytes(0x8A00))
des=base[:]; staged=base[:]
for c in range(2):
 st=0x6F44+c*0x148
 for rel in range(0x0C,0xDE,2): swap2(des,st+rel)
 for rel in range(0x0C,0xDE,2):
  if st+rel not in user_s2: swap2(staged,st+rel)
for o in user_s2:
 if any(0x6F44+c*0x148+0x0C<=o<0x6F44+c*0x148+0xDE for c in range(2)): swap2(staged,o)
for c in range(2):
 st=0x6F44+c*0x148
 assert staged[st+0x0C:st+0xDE]==des[st+0x0C:st+0xDE]
# card equivalence only arena table
base=bytearray(rnd.randbytes(98*0xE00))
des=base[:]; staged=base[:]
for k in range(98*110):
 slot,row=divmod(k,110); arena4(des,slot*0xE00+0x9B4+row*4)
for k in missing_keys:
 slot,row=divmod(k,110); arena4(staged,slot*0xE00+0x9B4+row*4)
for off in card_a: arena4(staged,off)
assert staged==des
# CEC packed slot equivalence
base=bytearray(rnd.randbytes(3*0xE00)); des=base[:]; staged=base[:]
for s in range(3):
 for row in range(110): arena4(des,s*0xE00+0x9B4+row*4)
for s in range(3):
 for row in range(110):
  if row not in cec_old: arena4(staged,s*0xE00+0x9B4+row*4)
 for row in cec_old: arena4(staged,s*0xE00+0x9B4+row*4)
assert staged==des
print('PASS')
print('shakalaka_missing=197')
print('guild_card_static=382 guild_card_missing=10398')
print('cec_static_per_slot=73 cec_missing_per_slot=37')
if len(sys.argv) > 1:
    exe_path = Path(sys.argv[1])
    exe = exe_path.read_bytes()
    assert exe[:2] == b'MZ' and b'mh3g-save-convert-core.exe' in exe
    print('wrapper_sha256='+hashlib.sha256(exe).hexdigest())
else:
    print('binary_check=skipped (source-only validation)')
