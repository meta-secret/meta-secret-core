import assert from 'node:assert/strict';
function span(s,a,b) { const i=s.indexOf(a),j=s.indexOf(b,i+a.length); assert(i>=0&&j>i,a); return s.slice(i,j); }
export function buildCreatePhaseFixture(two,secrets,shared) {
 const entry=span(two,'    func createVaultAndWaitForJoins()','    private func skipOnboardingIfNeeded(');
 const wrappers=span(two,'    private func waitForVisible(','    private func waitForText(');
 const intent=span(shared,'    func lbWaitIntent(','    func lbTapIntent(');
 const passive=span(secrets,'func liveboardPassiveSecretReachable(','\n@MainActor');
 const supported=secrets.includes('    func lbWaitForCreatedSecret(');
 const local=supported?span(secrets,'    func lbWaitForCreatedSecret(','    func lbFindSecret('):'';
 const ports=String.raw`import Foundation
import CoreGraphics
@MainActor enum Clock { static var now: TimeInterval=0 }
@MainActor struct Date {
 let tick: TimeInterval; init(){tick=Clock.now}; private init(_ t:TimeInterval){tick=t}
 func addingTimeInterval(_ t:TimeInterval)->Date{Date(tick+t)}
 var timeIntervalSinceNow:TimeInterval{tick-Clock.now}
 static func <(a:Date,b:Date)->Bool{a.tick<b.tick}
}
@MainActor enum AssertionPort { static var failures=0 }
@MainActor func XCTFail(_ s:String){AssertionPort.failures+=1}
@MainActor func XCTAssertTrue(_ b:Bool,_ s:String=""){if !b{XCTFail(s)}}
@MainActor func XCTAssertEqual<T:Equatable>(_ a:T,_ b:T,_ s:String=""){if a != b{XCTFail(s)}}
enum HostType { case any,other,window }
protocol XCUIElementSnapshot { var identifier:String{get};var label:String{get};var frame:CGRect{get};var elementType:HostType{get};var children:[any XCUIElementSnapshot]{get} }
final class Snapshot:XCUIElementSnapshot {
 let identifier:String,label:String,frame:CGRect,elementType:HostType,children:[any XCUIElementSnapshot]
 init(_ id:String,_ f:CGRect,_ c:[any XCUIElementSnapshot]=[],label:String="",type:HostType = .other){identifier=id;frame=f;children=c;self.label=label;elementType=type}
}
@MainActor struct Incoming {let kind:String}
@MainActor struct LiveboardIncomingExpected {let kind:String;var subject:String?=nil}
@MainActor final class Selection {var incoming:Incoming?;var secrets:[String:String]=[:];var secretId:String?}
@MainActor struct Element {let exists:Bool}
@MainActor struct HostQuery {
 let app:XCUIApplication;var id:String?=nil
 func matching(identifier:String)->HostQuery{HostQuery(app:app,id:identifier)}
 var count:Int{precondition(id=="lb-create");return app.readModalCount()}
}
@MainActor final class XCUIApplication {
 let mode:String,expectedNames=["fixture-one","fixture-two","fixture-three"];let lbSelection=Selection()
 var created:[String]=[],pendingName="",events:[String]=[],markers:[String]=[],batchSizes:[Int]=[],sameNames:[Bool]=[]
 var opens=0,submits=0,fills=0,approvals=0,fullScans=0,batches=0,singleScans=0,snapshots=0,routes=0,actions=0,hidden=0,initialScans = -1
 var budgets:[Double]=[],preBatchMappings:[Int]=[]
 var modalReads=0,modalWithinCreate=0,initialSnapshots = -1,modalCounts:[Int]=[]
 func descendants(matching:HostType)->HostQuery{HostQuery(app:self)}
 func readModalCount()->Int{
  modalReads+=1;modalWithinCreate+=1;events.append("MODAL_COUNT")
  if mode=="modal-late-first" && modalWithinCreate==1{Clock.now+=31}
  if mode=="modal-late-final" && modalWithinCreate==2{Clock.now+=31}
  let count:Int
  if mode=="modal-duplicate"{count=2}
  else if ["modal-present","modal-offscreen","modal-wait-return"].contains(mode){count=1}
  else if mode=="modal-reappears"{count=modalWithinCreate==1 ? 0 : 1}
  else if mode=="modal-delayed"{count=modalWithinCreate<=2 ? 1 : 0}
  else{count=0}
  modalCounts.append(count);return count
 }
 init(_ m:String){mode=m}
 func orbitEnterVaultName(_ s:String){};func orbitCreateVault(){}
 func lbSubmitCreate(){submits+=1;created.append(pendingName);modalWithinCreate=0;events.append("SUBMIT")}
 func lbTapIntent(_ id:String,timeout:TimeInterval){if id=="add-secret-fab"{opens+=1;events.append("OPEN_CREATE")}else if id=="tab-secrets"{events.append("POST_JOIN_ROUTE")}}
 func lbHome(timeout:TimeInterval){};func lbSecrets(timeout:TimeInterval){routes+=1}
 func lbVisible(_ e:Element)->Bool{e.exists}
 func lbElement(_ s:String)->Element{if s=="secret-name-input"{hidden+=1};return Element(exists:false)}
 func lbIncomingTerminal(_ i:Incoming,timeout:TimeInterval){}
 func lbIncoming(_ i:LiveboardIncomingExpected,timeout:TimeInterval)->Incoming{Incoming(kind:i.kind)}
 func lbWait(_ p:@escaping()->Bool,timeout:TimeInterval=180,message:String=""){
  budgets.append(timeout);if mode=="modal-wait-return"{return};for _ in 0..<3{if p(){return};Clock.now+=0.1};Clock.now+=max(0,timeout);XCTFail(message)
 }
 func lbFindSecret(_ n:String,timeout:TimeInterval)->String{fullScans+=1;singleScans+=1;events.append("FULL_SINGLE");lbSelection.secrets[n]="id-"+n;return "id-"+n}
 func lbFindSecrets(_ names:[String],timeout:TimeInterval)->[String:String]{
  preBatchMappings.append(lbSelection.secrets.count);fullScans+=1;batches+=1;events.append("FULL_BATCH");batchSizes.append(names.count);sameNames.append(names==expectedNames)
  var r=Dictionary(uniqueKeysWithValues:names.map{($0,"id-"+$0)})
  if mode=="global-missing" || (mode=="postjoin-missing" && batches==2){r.removeValue(forKey:names.last ?? "")}
  if mode=="global-duplicate"{r=Dictionary(uniqueKeysWithValues:names.map{($0,"same-id")})}
  if mode=="global-empty-id"{r=Dictionary(uniqueKeysWithValues:names.map{($0,"")})}
  if mode=="global-unproven"{r=[:]};lbSelection.secrets=r;return r
 }
 func lbSecretsViewportSnapshot()throws->(root:any XCUIElementSnapshot,bounds:CGRect){
  snapshots+=1;events.append("LOCAL_SNAPSHOT");if mode=="snapshot-error"{throw HostError.snapshot};if mode=="late-snapshot"{Clock.now=181}
  let app=CGRect(x:0,y:0,width:400,height:800),clip=CGRect(x:10,y:100,width:380,height:500)
  var nodes:[any XCUIElementSnapshot]=[Snapshot("lb-secrets",clip),Snapshot("lb-secrets-list",clip)],foreign:[any XCUIElementSnapshot]=[]
  for (i,n) in created.enumerated(){
   var f=CGRect(x:20,y:CGFloat(120+120*i),width:360,height:100);if mode=="offclip"{f.origin.y=900};if mode=="nonfinite"{f.origin.x = .nan}
   let id=mode=="empty-id" ? "" : "id-"+n
   let label=Snapshot("lb-secret-name-"+id,f,label:n),card=Snapshot("lb-secret-"+id,f)
   if mode=="foreign-window"{foreign += [label,card]}else{if !["local-refuse","missing-card"].contains(mode){nodes.append(card)};nodes.append(label)}
   if mode=="duplicate-name"{nodes.append(Snapshot("lb-secret-name-other",f,label:n))}
   if mode=="duplicate-card"{nodes.append(card)}
  }
  if mode=="modal-open"{nodes.append(Snapshot("lb-create",clip))}
  if mode=="wide-frontier"{nodes += (0..<20000).map{_ in Snapshot("",app)}}
  var windows:[any XCUIElementSnapshot]=[Snapshot("",app,nodes,type:.window)]
  if !foreign.isEmpty{windows.append(Snapshot("",app,foreign,type:.window))}
  return (Snapshot("",app,windows),clip)
 }
}
enum HostError:Error{case snapshot}
`;
 const owning='import Foundation\nimport CoreGraphics\n'+passive+'\n@MainActor extension XCUIApplication{\n'+intent+local+'\n}\n';
 const callers=String.raw`import Foundation
@MainActor struct SecretDefinition{let name:String;let value:String}
@MainActor final class CaseTwoCaller{
 let app:XCUIApplication;init(_ a:XCUIApplication){app=a}
 func configuredSecrets()->[SecretDefinition]{app.expectedNames.map{SecretDefinition(name:$0,value:"fixture-value")}}
 func coordinatorVaultName(defaultValue:String)->String{"fixture-vault"}
 func typeSecretNameAndValue(name:String,value:String)->Bool{app.fills+=1;app.pendingName=name;return true}
 func tap(_ id:String,timeout:TimeInterval=30){app.lbTapIntent(id,timeout:timeout)}
 func enterSimulatorPasscodeIfNeeded(){};func env(_ s:String,defaultValue:String)->String{defaultValue}
 func approvePendingJoin(marker:String){app.approvals+=1;app.events.append("JOIN")}
 func print(_ m:String){app.markers.append(m);if m=="E2E: IOS_INITIATOR_READY"{app.initialScans=app.fullScans;app.initialSnapshots=app.snapshots;app.events.append("INITIAL_READY")};if m=="E2E: IOS_SECRETS_READY"{app.events.append("SECRETS_READY")}}
${entry}
${wrappers}
}
`;
 const localMain='';
 const main=String.raw`import Foundation
MainActor.assumeIsolated{
 var flows:[[String:Any]]=[],locals:[[String:Any]]=[]
 for mode in ["plain","modal-delayed","modal-present","modal-duplicate","modal-offscreen","modal-reappears","modal-wait-return","modal-late-first","modal-late-final","global-missing","global-duplicate","global-empty-id","global-unproven","postjoin-missing"]{
  Clock.now=0;AssertionPort.failures=0;let app=XCUIApplication(mode);try! CaseTwoCaller(app).createVaultAndWaitForJoins()
  flows.append(["mode":mode,"failures":AssertionPort.failures,"opens":app.opens,"submits":app.submits,"fills":app.fills,"created":app.created.count,"approvals":app.approvals,"fullScans":app.fullScans,"singleScans":app.singleScans,"batches":app.batches,"initialScans":app.initialScans,"initialReady":app.markers.contains("E2E: IOS_INITIATOR_READY"),"secretsReady":app.markers.contains("E2E: IOS_SECRETS_READY"),"events":app.events,"batchSizes":app.batchSizes,"sameNames":app.sameNames,"preBatchMappings":app.preBatchMappings,"hidden":app.hidden,"initialSnapshots":app.initialSnapshots,"modalReads":app.modalReads,"modalCounts":app.modalCounts,"budgets":app.budgets])
 }
${localMain}
 let output:[String:Any]=["flows":flows,"locals":locals,"helperSupported":${supported?'true':'false'}]
 Swift.print(String(data:try! JSONSerialization.data(withJSONObject:output,options:[.sortedKeys]),encoding:.utf8)!)
}
`;
 return {ports,owning,callers,main};
}
