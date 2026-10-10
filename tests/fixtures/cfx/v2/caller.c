#include "shr_fx.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <stdio.h>
#define OK(x) assert((x)==0)
static shr_fx_status_v2 status(void *h) {
 shr_fx_status_v2 s; OK(shr_fx_v2_status(h,&s,2,sizeof s)); return s;
}
int main(void) {
 _Static_assert(sizeof(shr_fx_settings_v2)==64,"settings layout");
 _Static_assert(sizeof(shr_fx_status_v2)==200,"status layout");
 _Static_assert(offsetof(shr_fx_status_v2,current)==72,"current offset");
 shr_fx_capabilities_v2 caps; OK(shr_fx_v2_capabilities(&caps,2,sizeof caps));
 assert(caps.algorithm_mask==14 && caps.sample_bits==64 && !caps.hardware_budget_available);
 assert(shr_fx_v2_capabilities(&caps,1,sizeof caps)==-1);
 for(uint32_t a=1;a<=3;a++) {
  shr_fx_settings_v2 s; OK(shr_fx_v2_defaults(a,&s,2,sizeof s));
  for(uint32_t id=1;id<=6;id++){shr_fx_parameter_v2 p;OK(shr_fx_v2_parameter(a,id,&p,2,sizeof p));assert(p.algorithm==a);}
  if(a==1){s.time_ms=10;s.amount=.5;s.damping=0;s.gain=1;}
  if(a==3){s.time_ms=10;s.amount=0;s.gain=1;}
  void *h=shr_fx_v2_create(8000,8192,&s);assert(h);
  double in[8192]={0},out[8192]; in[0]=.5+ldexp(1,-40);
  OK(shr_fx_v2_process(h,in,out,4096));assert(out[0]==0);
  for(int i=0;i<4096;i++)assert(out[2*i+1]==0 && isfinite(out[2*i]));
  if(a==1){assert(out[160]==in[0]);assert(out[320]==in[0]*.5);}
  if(a==3)assert(out[160]==in[0]);
  if(a==2){/* Room: 1-frame predelay + rounded shortest 29.7ms comb;
             two allpasses each contribute -.5 on their first sample. */
   for(int i=0;i<239;i++)assert(out[i*2]==0);
   assert(fabs(out[478]-in[0]*(1-(.45+.5*.5))*.25*.25*.5)<1e-17);
  }
  shr_fx_settings_v2 edit=s;edit.gain=.25;void *p=shr_fx_v2_prepare(8000,8192,&edit);assert(p);
  assert(shr_fx_v2_publish(h,p,1,99)==-4);assert(status(h).current.gain==s.gain);
  OK(shr_fx_v2_publish(h,p,1,0));assert(status(h).accepted_request==1 && status(h).applied_request==0);
  for(int i=0;i<8192;i++)in[i]=0;
  OK(shr_fx_v2_process(h,in,out,4096));assert(status(h).applied_request==1 && status(h).current.gain==.25);
  shr_fx_v2_cancel(shr_fx_v2_retire(h));
  p=shr_fx_v2_prepare(8000,8192,&s);assert(shr_fx_v2_publish(h,p,1,1)==-5);shr_fx_v2_cancel(p);
  OK(shr_fx_v2_reset(h));OK(shr_fx_v2_process(h,in,out,4096));for(int i=0;i<8192;i++)assert(out[i]==0);
  edit.time_ms=NAN;assert(!shr_fx_v2_prepare(8000,8192,&edit));
  in[3]=NAN;assert(shr_fx_v2_process(h,in,out,4096)==-3);for(int i=0;i<8192;i++)assert(out[i]==0);
  assert(status(h).fault==1);OK(shr_fx_v2_reset(h));
  /* Quiesced destruction with pending state. */
  p=shr_fx_v2_prepare(8000,8192,&s);OK(shr_fx_v2_publish(h,p,2,1));shr_fx_v2_destroy(h);
 }
 puts("v2 C caller: wet samples, descriptors, edits and recovery passed");
 return 0;
}
