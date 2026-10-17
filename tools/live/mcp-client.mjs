// An actual MCP SDK client. Point MGLIVE_MCP_CLIENT at the installed
// @modelcontextprotocol/client package; its location is local configuration.
import fs from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import net from 'node:net';
const [executable,mg,out,descriptor='',mode='discovery',areaName='']=process.argv.slice(2);
if(!executable||!mg||!out||!process.env.MGLIVE_MCP_CLIENT) throw Error('Usage: mcp-client.mjs MCP MG OUT [DESCRIPTOR] [discovery|morning] [AREA]; set MGLIVE_MCP_CLIENT');
const sdk=process.env.MGLIVE_MCP_CLIENT;
const {Client,StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(sdk,'dist/index.mjs')));
const {StdioClientTransport}=await import(pathToFileURL(path.join(sdk,'dist/stdio.mjs')));
const sdkPackage=JSON.parse(await fs.readFile(path.join(sdk,'package.json'),'utf8'));
const args=['--mg',mg,'--roots',process.cwd()];if(descriptor)args.push('--descriptor',descriptor);
const environment={...process.env,NWN_ROOT:path.join(process.cwd(),'target/test-output/live-redis/no-native-game')};
const result={client:`${sdkPackage.name}@${sdkPackage.version}`,mode,passed:false,transports:{}};
await fs.mkdir(out,{recursive:true});
const evidence=path.join(out,mode==='morning'?'mcp-morning.json':'mcp-discovery.json');
let child,latestArea,latestStatus;
async function check(transport,label){
  const client=new Client({name:'live-acceptance',version:'1.0.0'},{capabilities:{}});
  try{
    await client.connect(transport);
    const listing=await client.listTools();
    const tools=listing.tools.map(t=>t.name).sort();
    for(const t of listing.tools){assert.equal(t.inputSchema.type,'object');assert.equal(t.inputSchema.additionalProperties,false);}
    const sessions=await client.callTool({name:'moonglow_sessions',arguments:{}});assert.equal(sessions.isError,false);
    const described=await client.callTool({name:'mg_describe',arguments:{}});assert.equal(described.isError,false);assert.equal(described.structuredContent.stdout.protocol,'moonglow-cli/1');
    result.transports[label]={tools,sessions:sessions.structuredContent,cliDescribe:true,server:client.getServerVersion()};
    if(descriptor){const status=await client.callTool({name:'live_status',arguments:{}});assert.equal(status.isError,false);result.transports[label].liveStatus=status.structuredContent;assert(sessions.structuredContent.sessions.length>0);}
    if(mode==='morning'&&label==='stdio'){
      const call=async(name,args)=>{const r=await client.callTool({name,arguments:args});assert.equal(r.isError,false,JSON.stringify(r));if(name==='live_area_state')latestArea=r.structuredContent;if(name==='live_status')latestStatus=r.structuredContent;return r.structuredContent;};
      const status=await call('live_status',{});const area=areaName||status.areas[0].area;
      const setup=await call('live_spawn',{area,id:'morning-mcp-setup',kind:'creature',r:'nw_bartender',x:25,y:25,z:0,f:0});
      const made=await call('live_wait',{id:setup.queued,timeout_ms:30000});assert.equal(made.ok,true);assert(made.obj);result.setup=made;
      let before,npc;
      for(let n=0;n<50;n++){before=await call('live_area_state',{area});npc=before.creatures.find(c=>c.id===made.obj);if(npc)break;if(n===49)throw Error('Spawned bartender was not reported');await new Promise(r=>setTimeout(r,200));}
      assert(npc.own&&!npc.pc&&!npc.dm&&!npc.possessed);result.target=npc.id;result.area=area;
      const operations=[['live_npc_move',{target:npc.id,x:20,y:20,z:0,run:true}],['live_npc_say',{target:npc.id,text:'Good morning!'}],['live_run_script',{target:npc.id,script:'mglive_probe'}],['live_run_chunk',{target:npc.id,code:'SetName(OBJECT_SELF, "Chunk ran"); AssignCommand(OBJECT_SELF, ActionJumpToLocation(Location(GetArea(OBJECT_SELF), Vector(25.0, 25.0, 0.0), 0.0))); SpeakString("Sent from outside");'}],['live_run_chunk',{code:'this is not nwscript'}]];
      result.commands=[];
      for(let i=0;i<operations.length;i++){
        const [name,input]=operations[i];const queued=await call(name,{area,id:`morning-mcp-${i}`, ...input});
        const completed=await call('live_wait',{id:queued.queued,timeout_ms:30000});
        assert.equal(completed.ok,i!==operations.length-1,JSON.stringify(completed));if(!completed.ok)assert(completed.err);
        result.commands.push({name,result:completed});
        if(name==='live_npc_move'){
          for(let n=0;n<100;n++){
            const a=await call('live_area_state',{area});const c=a.creatures.find(c=>c.id===npc.id);
            if(c&&Math.hypot(c.x-20,c.y-20)<1){result.walk={from:{x:npc.x,y:npc.y},to:{x:c.x,y:c.y},arrived:true};break;}
            if(n===99)throw Error('NPC did not reach the requested destination');
            await new Promise(r=>setTimeout(r,200));
          }
        }
        if(name==='live_run_script'){
          for(let n=0;n<30;n++){const a=await call('live_area_state',{area});if(a.creatures.find(c=>c.id===npc.id)?.name==='Probe ran')break;if(n===29)throw Error('Probe name did not appear');await new Promise(r=>setTimeout(r,200));}
        }
      }
      let after;
      for(let n=0;n<50;n++){after=await call('live_area_state',{area});const c=after.creatures.find(c=>c.id===npc.id);if(c?.name==='Chunk ran'&&Math.hypot(c.x-25,c.y-25)<1)break;if(n===49)throw Error('Chunk name/position did not appear');await new Promise(r=>setTimeout(r,200));}
      result.target=npc.id;result.area=area;
      await fs.writeFile(path.join(out,'mcp-before.json'),JSON.stringify(before,null,2));
      await fs.writeFile(path.join(out,'mcp-after.json'),JSON.stringify(after,null,2));
      await fs.writeFile(path.join(out,'status-mcp.json'),JSON.stringify(await call('live_status',{}),null,2));
      const teardown=await call('live_destroy',{area,id:'morning-mcp-teardown',target:npc.id});
      result.teardown=await call('live_wait',{id:teardown.queued,timeout_ms:30000});assert.equal(result.teardown.ok,true);
      for(let n=0;n<50;n++){const a=await call('live_area_state',{area});if(!a.creatures.some(c=>c.id===npc.id))break;if(n===49)throw Error('Owned MCP acceptance actor remained reported after destroy');await new Promise(r=>setTimeout(r,200));}
    }
  }finally{await client.close();}
}
try{
  await check(new StdioClientTransport({command:executable,args,env:environment,stderr:'inherit'}),'stdio');
  const reservation=net.createServer();await new Promise(r=>reservation.listen(0,'127.0.0.1',r));const port=reservation.address().port;await new Promise(r=>reservation.close(r));
  const token=(await import('node:crypto')).randomBytes(32).toString('hex');
  child=spawn(executable,[...args,'--http',`127.0.0.1:${port}`,'--http-token',token],{env:environment,windowsHide:true,stdio:['ignore','ignore','pipe']});
  await new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(Error('HTTP adapter did not start')),10000);child.once('error',reject);child.stderr.on('data',b=>{if(b.toString().includes('listening')){clearTimeout(timeout);resolve();}});});
  await check(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`),{requestInit:{headers:{Authorization:`Bearer ${token}`}}}),'http');
  assert.deepEqual(result.transports.stdio.tools,result.transports.http.tools);
  result.passed=true;
}catch(e){result.error=e.message;if(latestArea)await fs.writeFile(path.join(out,'mcp-failed-area.json'),JSON.stringify(latestArea,null,2));if(latestStatus)await fs.writeFile(path.join(out,'status-mcp-failed.json'),JSON.stringify(latestStatus,null,2));throw e;}
finally{if(child&&!child.killed){child.kill();await new Promise(r=>child.once('exit',r));}await fs.writeFile(evidence,JSON.stringify(result,null,2));}
console.log(JSON.stringify({passed:result.passed,client:result.client,tools:result.transports.stdio.tools.length,transports:Object.keys(result.transports)}));
