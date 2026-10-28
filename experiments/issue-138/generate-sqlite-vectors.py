"""Independent storage protocol vectors; imports no production implementation."""
import json
from pathlib import Path
schema="CREATE TABLE link_cli_log (sequence INTEGER PRIMARY KEY CHECK(sequence>0), record BLOB NOT NULL CHECK(typeof(record)='blob'))"
values=['', 'one 雪\0😀', 'engine2|1|0|2|+1,0,0,-|check', 'delimiters ;|,? =', 'combining e\u0301', '\ufeffleading BOM', '\ufeff\ufeff', '\U0010ffff']
vectors={
 'format':'link-cli-sqlite-log-v1',
 'applicationId':0x4c4e4b4a,
 'userVersion':1,
 'minimumSqliteVersion':'3.51.3',
 'maximumSequence':'9223372036854775807',
 'schema':schema,
 'records':[{'text':text,'utf8Hex':text.encode().hex()} for text in values],
 'invalidRecords':['\n','\r','one\ntwo','one\rtwo'],
 'invalidUtf8Hex':['ff','c080','eda080','f4908080'],
 'validSequences':[[],['1'],['1','2','3']],
 'invalidSequences':[['0'],['-1'],['2'],['1','3'],['1','2','4']],
 'invalidSchemas':[
  'CREATE TABLE link_cli_log (sequence INTEGER PRIMARY KEY, record BLOB)',
  "CREATE TABLE link_cli_log (sequence INTEGER PRIMARY KEY CHECK(sequence>0), record TEXT NOT NULL)",
  'CREATE TABLE link_cli_log (record BLOB, sequence INTEGER)',
 ],
 'defaultOptions':{'busyTimeoutMs':0,'autoCheckpointPages':1000,'csharpCommandTimeoutSeconds':1},
}
Path('docs/protocol/sqlite-log-vectors.json').write_text(json.dumps(vectors,ensure_ascii=False,indent=2)+'\n')
print(vectors['applicationId'])
