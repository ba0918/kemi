// @ts-check
/**
 * @typedef {{x:number,y:number}} Point
 * @typedef {Point & {width:number,height:number}} Box
 * @typedef {Box & {id:string,kind:string}} Shape
 * @typedef {{lines:string[],width:number,height:number,original:string}} TextBlock
 * @typedef {{scale:number,width:(text:string)=>number,wrap:(text:string,limit:number)=>TextBlock,dispose:()=>void}} Metrics
 * @typedef {{element:SVGTextElement,block:TextBlock,x:number,y:number,owner:string|null}} Label
 * @typedef {{points:Point[],head:Point[],endpoints:string[]}} Arrow
 * @typedef {{svg:SVGSVGElement,shapes:Shape[],labels:Label[],arrows:Arrow[],frames:Box[],width:number,height:number}} DrawingResult
 * @typedef {{svg:SVGSVGElement,metrics:Metrics,shapes:Shape[],labels:Label[],arrows:Arrow[],frames:Box[],text:(block:TextBlock,x:number,y:number,attrs?:Record<string,string|number|boolean>,owner?:string|null)=>SVGTextElement,arrow:(points:Point[],attrs?:Record<string,string|number|boolean>,endpoints?:string[])=>void,shape:(value:Shape,element:SVGElement)=>void,finish:(canvas?:{width:number,height:number})=>DrawingResult}} Drawing
 * @typedef {{type:'message',from:string,to:string,label:string,kind:'call'|'return',gap_after?:number}} Message
 * @typedef {{condition:string,messages:Message[]}} Branch
 * @typedef {{type:'sequence',title:string,role:string,participants:{id:string,label:string,x?:number}[],events:(Message|{type:'alt',branches:Branch[],gap_after?:number}|{type:'loop',condition:string,messages:Message[],gap_after?:number})[],layout?:{participant_gap?:number,event_gap?:number,self_loop_width?:number},canvas?:{width:number,height:number}}} Sequence
 * @typedef {{type:'flow',title:string,role:string,nodes:{id:string,kind:string,label:string,position?:Point,width?:number,height?:number}[],edges:{from:string,to:string,from_port?:string,to_port?:string,via?:Point[],label?:{text:string,position?:Point}}[],canvas?:{width:number,height:number}}} Flow
 * @typedef {Sequence|Flow|{type:'text',body:string}|{type:'code',body:string,language:string,role:string,title?:string}|{type:'diagram',source:string,title:string,role:string}} Part
 */
export {};
