// Revisions are local to a document; generation is owned by the Rust session.
export function compareDocumentSnapshot(current,next){
 if(!current)return 'replace';
 const a=current.generation??0,b=next.generation??0;
 if(b<a)return 'ignore';
 if(b>a)return 'replace';
 return next.revision<current.revision?'ignore':'update';
}
