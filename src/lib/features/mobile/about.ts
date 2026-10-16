/** Native About actions use a host gateway; construction never opens links. */
export const ABOUT_IDENTITY = { author:'ZeroDenet', declaredIdentifier:'org.zerodenet.znetsink' } as const;
export const ABOUT_LINKS = {
  website:{label:'官网',url:'https://zerodenet.org'},
  repository:{label:'代码仓库',url:'https://github.com/zerodenet/znet-sink'},
  feedback:{label:'问题反馈',url:'https://github.com/zerodenet/znet-sink/issues'},
} as const;
export type AboutLink = keyof typeof ABOUT_LINKS;
export interface AboutGateway {
  name():Promise<string>; version():Promise<string>; identifier():Promise<string>;
  open(url:string):Promise<void>; copy(text:string):Promise<void>;
}
export interface AboutView {
  loading:boolean; name:string|null; version:string|null; identifier:string|null;
  errors:string[]; opening:AboutLink|null; failedLink:AboutLink|null;
  copying:boolean; linkMessage:string|null;
}
export const unavailableKernelVersions = {
  running:{version:null,reason:'平台尚未提供真实运行内核的版本回执。'},
  bundled:{version:null,reason:'随包 Zero 运行产物尚未接入，不能提供产物版本。'},
} as const;
export class MobileAboutController {
  view:AboutView={loading:false,name:null,version:null,identifier:null,errors:[],opening:null,failedLink:null,copying:false,linkMessage:null};
  private generation=0;
  private disposed=false;
  private gateway:AboutGateway;
  private changed:(view:AboutView)=>void;
  constructor(gateway:AboutGateway,changed:(view:AboutView)=>void) {this.gateway=gateway;this.changed=changed;}
  private publish(patch:Partial<AboutView>){if(!this.disposed){this.view={...this.view,...patch};this.changed(this.view);}}
  dispose(){this.disposed=true;this.generation++;}
  async load(){
    if(this.disposed || this.view.loading)return;
    const generation=++this.generation;this.publish({loading:true,errors:[]});
    const results=await Promise.allSettled([this.gateway.name(),this.gateway.version(),this.gateway.identifier()]);
    if(this.disposed || generation!==this.generation)return;
    const labels=['应用名称','客户端版本','应用标识'];
    const values=results.map(result=>result.status==='fulfilled' && typeof result.value==='string' && result.value.trim() ? result.value.trim():null);
    const errors=values.flatMap((value,index)=>value ? []:[`${labels[index]}读取失败，请重新核对。`]);
    if(values[2] && values[2]!==ABOUT_IDENTITY.declaredIdentifier)errors.push('当前应用标识与项目声明不一致，请核对安装来源。');
    this.publish({loading:false,name:values[0],version:values[1],identifier:values[2],errors});
  }
  async open(link:AboutLink){
    if(this.disposed || this.view.opening || this.view.copying || !Object.hasOwn(ABOUT_LINKS,link))return;
    this.publish({opening:link,failedLink:null,linkMessage:null});
    try{await this.gateway.open(ABOUT_LINKS[link].url);this.publish({linkMessage:'已交给系统浏览器。'});}
    catch{this.publish({failedLink:link,linkMessage:'系统浏览器未能打开，可复制地址后打开。'});}
    finally{this.publish({opening:null});}
  }
  async copyFailedLink(){
    const link=this.view.failedLink;
    if(this.disposed || !link || this.view.copying || this.view.opening)return;
    this.publish({copying:true});
    try{await this.gateway.copy(ABOUT_LINKS[link].url);this.publish({linkMessage:'地址已复制。'});}
    catch{this.publish({linkMessage:'复制失败，地址仍保留，可选择文字手动复制。'});}
    finally{this.publish({copying:false});}
  }
}
